//! Windows CPU, memory, uptime, filesystem, configured Docker and NVIDIA readings.
//!
//! Published fields follow the Go collector: system CPU deltas, physical
//! memory plus page-file swap, a `windows major.minor.build` platform string,
//! tick-count uptime, and logical-drive usage. Load averages stay unset.
//! NVIDIA metrics use bounded `nvidia-smi` output; other Windows display and
//! compute devices are not yet enumerated.

use crate::{CpuTimes, DiskUsage, FilesystemMetrics, HostInfo, MemoryMetrics, Partition, Probe};
use std::{io, path::Path};

#[derive(Clone, Debug, Default)]
pub struct WindowsProbe;

impl Probe for WindowsProbe {
    fn engine(
        &self,
        host: Option<&str>,
        include_disk: bool,
    ) -> io::Result<Option<crate::ContainerEngine>> {
        // Windows has no implicit Unix socket. Match the Go collector: a
        // configured Docker host enables probing; an unset host is absent.
        let host = host
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                std::env::var("DOCKER_HOST")
                    .ok()
                    .filter(|value| !value.is_empty())
            });
        host.map_or(Ok(None), |host| {
            crate::docker::probe(Some(&host), include_disk)
        })
    }

    fn accelerators(&self) -> io::Result<(Vec<crate::GpuMetrics>, Vec<crate::AcceleratorMetrics>)> {
        Ok((
            nvidia_gpus(&crate::accel::SystemTools::default()),
            Vec::new(),
        ))
    }

    fn cpu_times(&self) -> io::Result<CpuTimes> {
        system_cpu_times()
    }

    fn load_average(&self) -> io::Result<[f64; 3]> {
        Err(unsupported("load averages unavailable on Windows"))
    }

    fn memory(&self) -> io::Result<MemoryMetrics> {
        system_memory()
    }

    fn host(&self) -> io::Result<HostInfo> {
        system_host()
    }

    fn partitions(&self) -> io::Result<Vec<Partition>> {
        system_partitions()
    }

    fn disk_usage(&self, mount: &Path) -> io::Result<DiskUsage> {
        system_disk_usage(mount)
    }
}

/// NVIDIA's bounded vendor tool supplies gauges and inventory on Windows
/// without depending on a Linux sysfs tree or a Windows display API wrapper.
pub fn nvidia_gpus(tools: &dyn crate::accel::ToolRunner) -> Vec<crate::GpuMetrics> {
    tools
        .run("nvidia-smi", &crate::accel::NVIDIA_ARGS)
        .map(|raw| crate::accel::parse_nvidia_csv(&raw))
        .unwrap_or_default()
}

/// `GetSystemTimes` reports idle inside the kernel total. The eight counters
/// match Linux `/proc/stat` order so [`CpuTimes::busy_percent_since`] applies.
pub fn cpu_times_from_system(idle: u64, kernel: u64, user: u64) -> CpuTimes {
    CpuTimes {
        values: [user, 0, kernel.saturating_sub(idle), idle, 0, 0, 0, 0],
    }
}

/// Physical memory comes from `GlobalMemoryStatusEx`. Swap is page-file pages
/// times the system page size; a missing page file leaves swap at zero.
pub fn memory_from_counters(
    total: u64,
    available: u64,
    swap_total: u64,
    swap_used: u64,
) -> MemoryMetrics {
    let mut out = MemoryMetrics {
        total_bytes: total,
        available_bytes: available,
        ..MemoryMetrics::default()
    };
    if total > 0 && available <= total {
        out.used_bytes = total - available;
        out.used_percent = out.used_bytes as f64 / total as f64 * 100.0;
    }
    if swap_total > 0 {
        out.swap_total_bytes = swap_total;
        out.swap_used_bytes = swap_used;
        out.swap_used_percent = (swap_used as f64 / swap_total as f64 * 100.0).clamp(0.0, 100.0);
    }
    out
}

/// `free_bytes` is the space available to the caller. Used space subtracts the
/// total free byte count, including space reserved from that caller.
pub fn disk_from_space(total: u64, available_to_caller: u64, total_free: u64) -> DiskUsage {
    let used = total.saturating_sub(total_free.min(total));
    let denominator = used.saturating_add(available_to_caller);
    DiskUsage {
        total_bytes: total,
        used_bytes: used,
        free_bytes: available_to_caller,
        used_percent: if denominator == 0 {
            0.0
        } else {
            used as f64 / denominator as f64 * 100.0
        },
    }
}

pub fn uptime_from_ticks(milliseconds: u64) -> i64 {
    i64::try_from(milliseconds / 1000).unwrap_or(i64::MAX)
}

pub fn platform_name(major: u32, minor: u32, build: u32) -> String {
    format!("windows {major}.{minor}.{build}")
}

/// `SystemDrive` has no trailing separator. The fallback mount is that drive
/// plus a backslash, or `\` when the variable is empty.
pub fn windows_root(system_drive: &str) -> String {
    let mut root = system_drive.to_owned();
    root.push('\\');
    root
}

/// NUL-separated UTF-16 strings, ending at the first empty entry.
pub fn split_multi_sz(buffer: &[u16]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < buffer.len() && buffer[start] != 0 {
        let end = buffer[start..]
            .iter()
            .position(|&unit| unit == 0)
            .map(|offset| start + offset)
            .unwrap_or(buffer.len());
        out.push(String::from_utf16_lossy(&buffer[start..end]));
        start = end + 1;
    }
    out
}

/// The system drive wins. The profile volume is the fallback. Comparison is
/// case-insensitive and treats `/` and `\` as the same separator.
pub fn mark_windows_primary(filesystems: &mut [FilesystemMetrics], system_drive: &str, home: &str) {
    let index =
        windows_holding(filesystems, system_drive).or_else(|| windows_holding(filesystems, home));
    if let Some(index) = index {
        filesystems[index].primary = true;
    }
}

fn windows_holding(filesystems: &[FilesystemMetrics], path: &str) -> Option<usize> {
    if path.is_empty() {
        return None;
    }
    let path = windows_key(path);
    let mut best: Option<(usize, usize)> = None;
    for (index, fs) in filesystems.iter().enumerate() {
        let mount = windows_key(&fs.mount);
        if holds_windows(&path, &mount) && best.is_none_or(|(_, len)| mount.len() > len) {
            best = Some((index, mount.len()));
        }
    }
    best.map(|(index, _)| index)
}

fn windows_key(value: &str) -> String {
    let trimmed = value.trim_end_matches(['\\', '/']);
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch == '/' {
            out.push('\\');
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

fn holds_windows(path: &str, mount: &str) -> bool {
    path == mount
        || (path.len() > mount.len()
            && path.as_bytes().get(mount.len()) == Some(&b'\\')
            && path.starts_with(mount))
}

fn unsupported(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}

#[cfg(windows)]
fn system_cpu_times() -> io::Result<CpuTimes> {
    let mut idle = sys::FileTime { low: 0, high: 0 };
    let mut kernel = sys::FileTime { low: 0, high: 0 };
    let mut user = sys::FileTime { low: 0, high: 0 };
    // SAFETY: each pointer refers to a writable FileTime local.
    let ok = unsafe { sys::GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    if ok == 0 {
        return Err(api_error("GetSystemTimes"));
    }
    Ok(cpu_times_from_system(
        file_time(idle),
        file_time(kernel),
        file_time(user),
    ))
}

#[cfg(windows)]
fn file_time(value: sys::FileTime) -> u64 {
    (u64::from(value.high) << 32) | u64::from(value.low)
}

#[cfg(windows)]
fn system_memory() -> io::Result<MemoryMetrics> {
    let mut status = sys::MemoryStatusEx {
        length: std::mem::size_of::<sys::MemoryStatusEx>() as u32,
        _load: 0,
        total_phys: 0,
        avail_phys: 0,
        _total_page: 0,
        _avail_page: 0,
        _total_virtual: 0,
        _avail_virtual: 0,
        _avail_extended: 0,
    };
    // SAFETY: status is writable and its length field is the struct size.
    let ok = unsafe { sys::GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return Err(api_error("GlobalMemoryStatusEx"));
    }
    let (swap_total, swap_used) = page_file_bytes().unwrap_or((0, 0));
    Ok(memory_from_counters(
        status.total_phys,
        status.avail_phys,
        swap_total,
        swap_used,
    ))
}

#[cfg(windows)]
fn page_file_bytes() -> io::Result<(u64, u64)> {
    let mut totals = PageTotals {
        total_pages: 0,
        used_pages: 0,
    };
    // SAFETY: EnumPageFilesW invokes collect_page_file synchronously with this
    // totals pointer. The callback only adds the page counts it is given.
    let ok =
        unsafe { sys::EnumPageFilesW(collect_page_file, std::ptr::from_mut(&mut totals).cast()) };
    if ok == 0 {
        return Err(api_error("EnumPageFilesW"));
    }
    let page = page_size();
    if page == 0 {
        return Ok((0, 0));
    }
    Ok((
        totals.total_pages.saturating_mul(page),
        totals.used_pages.saturating_mul(page),
    ))
}

#[cfg(windows)]
struct PageTotals {
    total_pages: u64,
    used_pages: u64,
}

#[cfg(windows)]
unsafe extern "system" fn collect_page_file(
    param: *mut std::ffi::c_void,
    info: *const sys::PageFileInfo,
    _name: *const u16,
) -> i32 {
    if param.is_null() || info.is_null() {
        return 0;
    }
    // SAFETY: param is the PageTotals passed to EnumPageFilesW, and info is
    // the page-file record the API supplies for this callback.
    unsafe {
        let totals = &mut *param.cast::<PageTotals>();
        let info = &*info;
        totals.total_pages = totals.total_pages.saturating_add(info.total as u64);
        totals.used_pages = totals.used_pages.saturating_add(info.used as u64);
    }
    1
}

#[cfg(windows)]
fn page_size() -> u64 {
    let mut info = sys::SystemInfo {
        _oem_id: 0,
        page_size: 0,
        _min_app: 0,
        _max_app: 0,
        _active_mask: 0,
        _processors: 0,
        _processor_type: 0,
        _allocation_granularity: 0,
        _level: 0,
        _revision: 0,
    };
    // SAFETY: info is a writable SystemInfo local.
    unsafe { sys::GetSystemInfo(&mut info) };
    u64::from(info.page_size)
}

#[cfg(windows)]
fn system_host() -> io::Result<HostInfo> {
    let mut info = sys::OsVersionInfoEx {
        size: std::mem::size_of::<sys::OsVersionInfoEx>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        _platform_id: 0,
        _csd: [0; 128],
        _service_pack_major: 0,
        _service_pack_minor: 0,
        _suite_mask: 0,
        _product_type: 0,
        _reserved: 0,
    };
    // SAFETY: info is writable and size is the full OSVERSIONINFOEXW length.
    // RtlGetVersion fills the version fields and does not fail in practice.
    unsafe {
        sys::RtlGetVersion(&mut info);
    }
    Ok(HostInfo {
        hostname: computer_name().unwrap_or_default(),
        platform: platform_name(info.major, info.minor, info.build),
        uptime_seconds: uptime_from_ticks(unsafe { sys::GetTickCount64() }),
    })
}

#[cfg(windows)]
fn computer_name() -> io::Result<String> {
    let mut size = 64_u32;
    for _ in 0..4 {
        let mut buf = vec![0_u16; size as usize];
        // SAFETY: buf is writable and size is its length in UTF-16 units.
        let ok = unsafe {
            sys::GetComputerNameExW(
                sys::COMPUTER_NAME_PHYSICAL_DNS_HOSTNAME,
                buf.as_mut_ptr(),
                &mut size,
            )
        };
        if ok != 0 {
            let len = (size as usize).min(buf.len());
            return Ok(String::from_utf16_lossy(&buf[..len]));
        }
        // SAFETY: read immediately after the failed computer-name call.
        let code = unsafe { sys::GetLastError() } as i32;
        if code != sys::ERROR_MORE_DATA || (size as usize) <= buf.len() || size > 32_768 {
            let err = io::Error::from_raw_os_error(code);
            return Err(io::Error::new(
                err.kind(),
                format!("GetComputerNameExW: {err}"),
            ));
        }
    }
    Err(io::Error::other(
        "GetComputerNameExW: computer name changed during repeated reads",
    ))
}

#[cfg(windows)]
fn system_partitions() -> io::Result<Vec<Partition>> {
    // SAFETY: a null buffer with length zero queries the required size.
    let needed = unsafe { sys::GetLogicalDriveStringsW(0, std::ptr::null_mut()) };
    if needed == 0 {
        return Err(api_error("list drives"));
    }
    let len = (needed as usize).saturating_add(1);
    if len > u32::MAX as usize {
        return Err(io::Error::other("read drives: drive list too large"));
    }
    let mut buf = vec![0_u16; len];
    // SAFETY: buf is writable and its length is passed in UTF-16 units.
    let wrote = unsafe { sys::GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
    if wrote == 0 || wrote as usize >= buf.len() {
        return Err(api_error("read drives"));
    }
    let mut out = Vec::new();
    for root in split_multi_sz(&buf) {
        let mut wide: Vec<u16> = root.encode_utf16().collect();
        wide.push(0);
        let mut fs_name = [0_u16; 256];
        // SAFETY: wide and fs_name are NUL-terminated writable buffers. Volume
        // name, serial, component length and flags are optional and null.
        let ok = unsafe {
            sys::GetVolumeInformationW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_name.as_mut_ptr(),
                fs_name.len() as u32,
            )
        };
        if ok != 0 {
            out.push(Partition {
                device: root.clone(),
                mount: root,
                fstype: utf16_c_string(&fs_name),
            });
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn system_disk_usage(mount: &Path) -> io::Result<DiskUsage> {
    let text = mount.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "encode drive: path is not Unicode",
        )
    })?;
    if text.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "encode drive: NUL in path",
        ));
    }
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let mut available = 0_u64;
    let mut total = 0_u64;
    let mut free = 0_u64;
    // SAFETY: wide is NUL-terminated and the three counters are writable.
    let ok =
        unsafe { sys::GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };
    if ok == 0 {
        // SAFETY: read immediately after the failed disk call.
        let code = unsafe { sys::GetLastError() } as i32;
        let err = io::Error::from_raw_os_error(code);
        return Err(io::Error::new(
            err.kind(),
            format!("read drive {text}: {err}"),
        ));
    }
    Ok(disk_from_space(total, available, free))
}

#[cfg(windows)]
fn utf16_c_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

#[cfg(windows)]
fn api_error(api: &str) -> io::Error {
    // SAFETY: callers invoke this immediately after the failed API call.
    let code = unsafe { sys::GetLastError() } as i32;
    let err = io::Error::from_raw_os_error(code);
    io::Error::new(err.kind(), format!("{api}: {err}"))
}

#[cfg(not(windows))]
fn system_cpu_times() -> io::Result<CpuTimes> {
    Err(unsupported("Windows CPU counters are unavailable"))
}

#[cfg(not(windows))]
fn system_memory() -> io::Result<MemoryMetrics> {
    Err(unsupported("Windows memory counters are unavailable"))
}

#[cfg(not(windows))]
fn system_host() -> io::Result<HostInfo> {
    Err(unsupported("Windows host information is unavailable"))
}

#[cfg(not(windows))]
fn system_partitions() -> io::Result<Vec<Partition>> {
    Err(unsupported("Windows drive enumeration is unavailable"))
}

#[cfg(not(windows))]
fn system_disk_usage(mount: &Path) -> io::Result<DiskUsage> {
    let _ = mount;
    Err(unsupported("Windows disk usage is unavailable"))
}

#[cfg(windows)]
mod sys {
    pub const COMPUTER_NAME_PHYSICAL_DNS_HOSTNAME: i32 = 5;
    pub const ERROR_MORE_DATA: i32 = 234;

    #[repr(C)]
    pub struct FileTime {
        pub low: u32,
        pub high: u32,
    }

    #[repr(C)]
    pub struct MemoryStatusEx {
        pub length: u32,
        pub _load: u32,
        pub total_phys: u64,
        pub avail_phys: u64,
        pub _total_page: u64,
        pub _avail_page: u64,
        pub _total_virtual: u64,
        pub _avail_virtual: u64,
        pub _avail_extended: u64,
    }

    #[repr(C)]
    pub struct PageFileInfo {
        pub _size: u32,
        pub _reserved: u32,
        pub total: usize,
        pub used: usize,
        pub _peak: usize,
    }

    #[repr(C)]
    pub struct SystemInfo {
        pub _oem_id: u32,
        pub page_size: u32,
        pub _min_app: usize,
        pub _max_app: usize,
        pub _active_mask: usize,
        pub _processors: u32,
        pub _processor_type: u32,
        pub _allocation_granularity: u32,
        pub _level: u16,
        pub _revision: u16,
    }

    #[repr(C)]
    pub struct OsVersionInfoEx {
        pub size: u32,
        pub major: u32,
        pub minor: u32,
        pub build: u32,
        pub _platform_id: u32,
        pub _csd: [u16; 128],
        pub _service_pack_major: u16,
        pub _service_pack_minor: u16,
        pub _suite_mask: u16,
        pub _product_type: u8,
        pub _reserved: u8,
    }

    const _: () = assert!(std::mem::size_of::<FileTime>() == 8);
    const _: () = assert!(std::mem::size_of::<MemoryStatusEx>() == 64);
    const _: () = {
        assert!(std::mem::size_of::<PageFileInfo>() == 8 + 3 * std::mem::size_of::<usize>());
    };
    const _: () = assert!(std::mem::size_of::<OsVersionInfoEx>() == 284);
    const _: () = {
        assert!(
            std::mem::size_of::<SystemInfo>()
                == if cfg!(target_pointer_width = "64") {
                    48
                } else {
                    36
                }
        );
    };

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetSystemTimes(
            idle: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        pub fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
        pub fn GetTickCount64() -> u64;
        pub fn GetLogicalDriveStringsW(length: u32, buffer: *mut u16) -> u32;
        pub fn GetVolumeInformationW(
            root: *const u16,
            volume_name: *mut u16,
            volume_name_size: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            fs_name: *mut u16,
            fs_name_size: u32,
        ) -> i32;
        pub fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
        pub fn GetComputerNameExW(format: i32, buffer: *mut u16, size: *mut u32) -> i32;
        pub fn GetSystemInfo(info: *mut SystemInfo);
        pub fn GetLastError() -> u32;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        pub fn EnumPageFilesW(
            callback: unsafe extern "system" fn(
                *mut std::ffi::c_void,
                *const PageFileInfo,
                *const u16,
            ) -> i32,
            context: *mut std::ffi::c_void,
        ) -> i32;
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        pub fn RtlGetVersion(info: *mut OsVersionInfoEx) -> i32;
    }
}
