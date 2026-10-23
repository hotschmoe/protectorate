//! Native user32/GDI desktop driver. All input is gated by Controller first.
use super::{typing::plan_text, Desktop, Error, Ready, Result, ScreenState};
use chrono::{DateTime, Utc};
use protectorate_desk_policy::{Chord, Key};
use serde_json::{json, Value};
use std::{
    ffi::c_void,
    mem::size_of,
    os::windows::process::CommandExt,
    process::Command,
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, OnceLock,
    },
    thread,
    time::Duration,
};

type Handle = isize;
type Hwnd = isize;
const UOI_NAME: i32 = 2;
const SM_XVIRTUALSCREEN: i32 = 76;
const SM_YVIRTUALSCREEN: i32 = 77;
const SM_CXVIRTUALSCREEN: i32 = 78;
const SM_CYVIRTUALSCREEN: i32 = 79;
const WH_KEYBOARD_LL: i32 = 13;
const WH_MOUSE_LL: i32 = 14;
const INPUT_MOUSE: u32 = 0;
const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_EXTENDEDKEY: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 2;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const TOKEN_QUERY: u32 = 8;
const TOKEN_INTEGRITY_LEVEL: u32 = 25;
const SRCCOPY: u32 = 0x00cc0020;
const CAPTUREBLT: u32 = 0x4000_0000;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    _pad: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
    private: u32,
}
#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    size_image: u32,
    xppm: i32,
    yppm: i32,
    used: u32,
    important: u32,
}
#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 3],
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    data: u32,
    flags: u32,
    time: u32,
    _pad: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct KeyInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    _pad: u32,
    extra: usize,
}
#[repr(C)]
union InputData {
    mouse: MouseInput,
    key: KeyInput,
}
#[repr(C)]
struct Input {
    kind: u32,
    _pad: u32,
    data: InputData,
}
const _: [(); 40] = [(); size_of::<Input>()];

#[link(name = "user32")]
unsafe extern "system" {
    fn SetProcessDPIAware() -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn GetDC(hwnd: Hwnd) -> Handle;
    fn ReleaseDC(hwnd: Hwnd, dc: Handle) -> i32;
    fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, data: isize) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn IsIconic(hwnd: Hwnd) -> i32;
    fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    fn GetWindowTextW(hwnd: Hwnd, buffer: *mut u16, len: i32) -> i32;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
    fn GetForegroundWindow() -> Hwnd;
    fn WindowFromPoint(point: Point) -> Hwnd;
    fn GetAncestor(hwnd: Hwnd, flags: u32) -> Hwnd;
    fn GetProcessWindowStation() -> Handle;
    fn GetUserObjectInformationW(
        handle: Handle,
        index: i32,
        data: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
    fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> Handle;
    fn CloseDesktop(handle: Handle) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SendInput(count: u32, input: *const Input, size: i32) -> u32;
    fn GetKeyboardLayout(thread: u32) -> Handle;
    fn VkKeyScanExW(ch: u16, layout: Handle) -> i16;
    fn MapVirtualKeyExW(code: u32, map_type: u32, layout: Handle) -> u32;
    fn GetKeyState(key: i32) -> i16;
    fn LockWorkStation() -> i32;
    fn SetWindowsHookExW(
        id: i32,
        callback: unsafe extern "system" fn(i32, usize, isize) -> isize,
        module: Handle,
        thread: u32,
    ) -> Handle;
    fn UnhookWindowsHookEx(handle: Handle) -> i32;
    fn CallNextHookEx(handle: Handle, code: i32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: Handle) -> Handle;
    fn CreateCompatibleBitmap(dc: Handle, width: i32, height: i32) -> Handle;
    fn SelectObject(dc: Handle, object: Handle) -> Handle;
    fn BitBlt(
        dest: Handle,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        src: Handle,
        src_x: i32,
        src_y: i32,
        rop: u32,
    ) -> i32;
    fn GetDIBits(
        dc: Handle,
        bitmap: Handle,
        start: u32,
        lines: u32,
        bits: *mut c_void,
        info: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
    fn DeleteObject(object: Handle) -> i32;
    fn DeleteDC(dc: Handle) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentProcess() -> Handle;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn GetTickCount64() -> u64;
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn QueryFullProcessImageNameW(
        handle: Handle,
        flags: u32,
        buffer: *mut u16,
        size: *mut u32,
    ) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        info: *mut c_void,
        length: u32,
        needed: *mut u32,
    ) -> i32;
}

static PHYSICAL_TICK: OnceLock<AtomicU64> = OnceLock::new();
static HOOK_READY: OnceLock<bool> = OnceLock::new();
static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);
unsafe extern "system" fn keyboard_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && lparam != 0 {
        // KBDLLHOOKSTRUCT.flags: bit 4 marks injected input.
        let flags = unsafe { *((lparam as *const u32).add(2)) };
        if flags & 0x10 == 0 {
            if let Some(tick) = PHYSICAL_TICK.get() {
                tick.store(unsafe { GetTickCount64() }, Ordering::Release);
            }
        }
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}
unsafe extern "system" fn mouse_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && lparam != 0 {
        // MSLLHOOKSTRUCT.flags: bit 0 marks injected input.
        let flags = unsafe { *((lparam as *const u32).add(3)) };
        if flags & 1 == 0 {
            if let Some(tick) = PHYSICAL_TICK.get() {
                tick.store(unsafe { GetTickCount64() }, Ordering::Release);
            }
        }
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}
fn start_input_monitor() -> bool {
    *HOOK_READY.get_or_init(|| {
        PHYSICAL_TICK.get_or_init(|| AtomicU64::new(unsafe { GetTickCount64() }));
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || unsafe {
            let module = GetModuleHandleW(ptr::null());
            let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, keyboard_hook, module, 0);
            let mouse = SetWindowsHookExW(WH_MOUSE_LL, mouse_hook, module, 0);
            let ready = keyboard != 0 && mouse != 0;
            HOOK_ACTIVE.store(ready, Ordering::Release);
            let _ = tx.send(ready);
            if ready {
                let mut msg = std::mem::MaybeUninit::<Msg>::zeroed();
                while GetMessageW(msg.as_mut_ptr(), 0, 0, 0) > 0 {}
            }
            if keyboard != 0 {
                UnhookWindowsHookEx(keyboard);
            }
            if mouse != 0 {
                UnhookWindowsHookEx(mouse);
            }
            HOOK_ACTIVE.store(false, Ordering::Release);
        });
        rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false)
    })
}

pub struct WindowsDesktop {
    monitor: bool,
}
impl WindowsDesktop {
    pub fn new() -> Self {
        unsafe {
            SetProcessDPIAware();
        }
        Self {
            monitor: start_input_monitor(),
        }
    }
}
fn win_error(what: &str) -> Error {
    Error::internal(format!("{what}: {}", std::io::Error::last_os_error()))
}
fn utf16_name(handle: Handle) -> Option<String> {
    let mut buf = [0u16; 256];
    let mut needed = 0;
    let ok = unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            buf.as_mut_ptr().cast(),
            (buf.len() * 2) as u32,
            &mut needed,
        )
    };
    (ok != 0).then(|| {
        String::from_utf16_lossy(&buf[..buf.iter().position(|v| *v == 0).unwrap_or(buf.len())])
    })
}
fn window_title(hwnd: Hwnd) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let got = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..got.max(0) as usize])
}
fn origin() -> (i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    }
}
fn size() -> (i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}
fn token_integrity(process: Handle) -> Option<u32> {
    let mut token = 0;
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let mut needed = 0;
    unsafe {
        GetTokenInformation(
            token,
            TOKEN_INTEGRITY_LEVEL,
            ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    let mut data = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TOKEN_INTEGRITY_LEVEL,
            data.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    unsafe {
        CloseHandle(token);
    }
    if ok == 0 || data.is_empty() {
        return None;
    }
    // TOKEN_MANDATORY_LABEL starts with SID_AND_ATTRIBUTES; SID pointer is first.
    let sid = data[0] as *const u8;
    if sid.is_null() {
        return None;
    }
    let count = unsafe { *sid.add(1) as usize };
    if count == 0 {
        return None;
    }
    Some(unsafe { *(sid.add(8 + (count - 1) * 4) as *const u32) })
}
fn integrity_name(level: u32) -> &'static str {
    match level {
        0x5000.. => "protected",
        0x4000.. => "system",
        0x3000.. => "high",
        0x2000.. => "medium",
        0x1000.. => "low",
        _ => "untrusted",
    }
}
fn window_info(hwnd: Hwnd) -> Value {
    let agent_level = token_integrity(unsafe { GetCurrentProcess() }).unwrap_or(0x2000);
    let mut pid = 0;
    if hwnd != 0 {
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
    }
    let mut process = String::new();
    let mut unreadable = false;
    let mut level = 0;
    if pid != 0 {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle == 0 {
            unreadable = true;
        } else {
            level = token_integrity(handle).unwrap_or_else(|| {
                unreadable = true;
                0
            });
            let mut buf = [0u16; 1024];
            let mut n = buf.len() as u32;
            if unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut n) } != 0 {
                let full = String::from_utf16_lossy(&buf[..n as usize]);
                process = full.rsplit('\\').next().unwrap_or(&full).to_owned();
            }
            unsafe {
                CloseHandle(handle);
            }
        }
    }
    json!({"title": window_title(hwnd), "pid": pid, "process": process,
        "integrity": if unreadable { "unreadable" } else { integrity_name(level) },
        "agentIntegrity": integrity_name(agent_level),
        "blocks": pid != 0 && (unreadable || level > agent_level)})
}
fn check_reach(hwnd: Hwnd) -> Result<()> {
    if window_info(hwnd)["blocks"] == true {
        return Err(Error::unavailable(
            "input blocked by a higher-integrity window",
            None,
        ));
    }
    Ok(())
}
fn require_unlocked() -> Result<()> {
    let desktop = unsafe { OpenInputDesktop(0, 0, 1) };
    if desktop == 0 {
        return Err(Error::unavailable(
            "input desktop is locked",
            Some(super::Reason::Locked),
        ));
    }
    let name = utf16_name(desktop);
    unsafe { CloseDesktop(desktop) };
    if name.is_some_and(|name| name.eq_ignore_ascii_case("default")) {
        Ok(())
    } else {
        Err(Error::unavailable(
            "input desktop is locked",
            Some(super::Reason::Locked),
        ))
    }
}
fn key_input(vk: u16, scan: u16, flags: u32) -> Input {
    Input {
        kind: INPUT_KEYBOARD,
        _pad: 0,
        data: InputData {
            key: KeyInput {
                vk,
                scan,
                flags,
                time: 0,
                _pad: 0,
                extra: 0,
            },
        },
    }
}
fn mouse_input(flags: u32) -> Input {
    Input {
        kind: INPUT_MOUSE,
        _pad: 0,
        data: InputData {
            mouse: MouseInput {
                dx: 0,
                dy: 0,
                data: 0,
                flags,
                time: 0,
                _pad: 0,
                extra: 0,
            },
        },
    }
}
fn send_inputs(inputs: &[Input]) -> Result<()> {
    if unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<Input>() as i32,
        )
    } != inputs.len() as u32
    {
        return Err(win_error(
            "SendInput (input may be blocked by a higher-integrity window)",
        ));
    }
    Ok(())
}
fn vk_events(key: Key, up: bool) -> Input {
    key_input(
        key.vk,
        0,
        (u32::from(key.extended) * KEYEVENTF_EXTENDEDKEY) | if up { KEYEVENTF_KEYUP } else { 0 },
    )
}

fn screen_point(x: i32, y: i32) -> Result<Point> {
    let (w, h) = size();
    if x < 0 || y < 0 || x >= w || y >= h {
        return Err(Error::invalid("pointer outside virtual screen"));
    }
    let (x0, y0) = origin();
    Ok(Point {
        x: x.checked_add(x0)
            .ok_or_else(|| Error::invalid("pointer outside virtual screen"))?,
        y: y.checked_add(y0)
            .ok_or_else(|| Error::invalid("pointer outside virtual screen"))?,
    })
}

impl Desktop for WindowsDesktop {
    fn ready(&self) -> Ready {
        let mut session = 0;
        if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 || session == 0
        {
            return Ready::NotInteractive;
        }
        let station = unsafe { GetProcessWindowStation() };
        if station == 0
            || !utf16_name(station).is_some_and(|name| name.eq_ignore_ascii_case("winsta0"))
        {
            return Ready::NotInteractive;
        }
        Ready::Interactive
    }
    fn screen(&self) -> ScreenState {
        let desktop = unsafe { OpenInputDesktop(0, 0, 1) };
        if desktop == 0 {
            return ScreenState::Locked;
        }
        let name = utf16_name(desktop);
        unsafe {
            CloseDesktop(desktop);
        }
        if name.is_some_and(|name| name.eq_ignore_ascii_case("default")) {
            ScreenState::Unlocked
        } else {
            ScreenState::Locked
        }
    }
    fn last_physical_input(&self) -> Option<DateTime<Utc>> {
        if !self.monitor || !HOOK_ACTIVE.load(Ordering::Acquire) {
            return None;
        }
        let last = PHYSICAL_TICK.get()?.load(Ordering::Acquire);
        let elapsed = unsafe { GetTickCount64() }.saturating_sub(last);
        Some(Utc::now() - chrono::Duration::milliseconds(elapsed.min(i64::MAX as u64) as i64))
    }
    fn screenshot(&self, max_width: i32) -> Result<Value> {
        let (x, y) = origin();
        let (w, h) = size();
        if w <= 0 || h <= 0 || w > 16_384 || h > 16_384 || i64::from(w) * i64::from(h) > 32_000_000
        {
            return Err(Error::unavailable("screen has invalid size", None));
        }
        let screen = unsafe { GetDC(0) };
        if screen == 0 {
            return Err(win_error("GetDC"));
        }
        let mem = unsafe { CreateCompatibleDC(screen) };
        let bitmap = if mem != 0 {
            unsafe { CreateCompatibleBitmap(screen, w, h) }
        } else {
            0
        };
        if mem == 0 || bitmap == 0 {
            if bitmap != 0 {
                unsafe {
                    DeleteObject(bitmap);
                }
            }
            if mem != 0 {
                unsafe {
                    DeleteDC(mem);
                }
            }
            unsafe {
                ReleaseDC(0, screen);
            }
            return Err(win_error("capture bitmap"));
        }
        let old = unsafe { SelectObject(mem, bitmap) };
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        let mut info = BitmapInfo {
            header: BitmapInfoHeader {
                size: size_of::<BitmapInfoHeader>() as u32,
                width: w,
                height: -h,
                planes: 1,
                bits: 32,
                compression: 0,
                size_image: 0,
                xppm: 0,
                yppm: 0,
                used: 0,
                important: 0,
            },
            colors: [0; 3],
        };
        let captured = unsafe { BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT) } != 0;
        // GetDIBits requires the bitmap to be deselected from its DC.
        unsafe { SelectObject(mem, old) };
        let copied = captured
            && unsafe {
                GetDIBits(
                    screen,
                    bitmap,
                    0,
                    h as u32,
                    pixels.as_mut_ptr().cast(),
                    &mut info,
                    0,
                )
            } != 0;
        unsafe {
            DeleteObject(bitmap);
            DeleteDC(mem);
            ReleaseDC(0, screen);
        }
        if !copied {
            return Err(win_error("capture screen"));
        }
        let out_w = if max_width > 0 { w.min(max_width) } else { w };
        let out_h = ((h as i64 * out_w as i64) / w as i64).max(1) as i32;
        let mut rgb = vec![0u8; out_w as usize * out_h as usize * 4];
        for yy in 0..out_h as usize {
            for xx in 0..out_w as usize {
                let sx = xx * w as usize / out_w as usize;
                let sy = yy * h as usize / out_h as usize;
                let src = (sy * w as usize + sx) * 4;
                let dst = (yy * out_w as usize + xx) * 4;
                rgb[dst..dst + 4].copy_from_slice(&[
                    pixels[src + 2],
                    pixels[src + 1],
                    pixels[src],
                    255,
                ]);
            }
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, out_w as u32, out_h as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| Error::internal(e.to_string()))?;
            writer
                .write_image_data(&rgb)
                .map_err(|e| Error::internal(e.to_string()))?;
        }
        use base64::Engine;
        Ok(
            json!({"png": base64::engine::general_purpose::STANDARD.encode(png_bytes),"width":w,"height":h}),
        )
    }
    fn windows(&self) -> Result<Value> {
        unsafe extern "system" fn enumerate(hwnd: Hwnd, data: isize) -> i32 {
            if unsafe { IsWindowVisible(hwnd) } == 0 {
                return 1;
            }
            let title = window_title(hwnd);
            if title.is_empty() {
                return 1;
            }
            let mut rect = Rect::default();
            if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
                return 1;
            }
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
            let (x, y) = origin();
            let out = unsafe { &mut *(data as *mut Vec<Value>) };
            out.push(json!({"title":title,"x":rect.left-x,"y":rect.top-y,
                "width":rect.right-rect.left,"height":rect.bottom-rect.top,"pid":pid,
                "minimized":unsafe { IsIconic(hwnd) } != 0}));
            1
        }
        let mut windows = Vec::new();
        if unsafe { EnumWindows(enumerate, (&mut windows as *mut Vec<Value>) as isize) } == 0 {
            return Err(win_error("EnumWindows"));
        }
        Ok(json!({"windows":windows}))
    }
    fn foreground(&self) -> Result<Value> {
        Ok(window_info(unsafe { GetForegroundWindow() }))
    }
    fn move_pointer(&self, x: i32, y: i32) -> Result<()> {
        require_unlocked()?;
        let point = screen_point(x, y)?;
        if unsafe { SetCursorPos(point.x, point.y) } == 0 {
            return Err(win_error("SetCursorPos"));
        }
        let mut actual = Point::default();
        if unsafe { GetCursorPos(&mut actual) } == 0 {
            return Err(win_error("GetCursorPos"));
        }
        if actual.x != point.x || actual.y != point.y {
            return Err(Error::unavailable("pointer held by another process", None));
        }
        Ok(())
    }
    fn click(&self, x: i32, y: i32, button: &str, double: bool) -> Result<()> {
        require_unlocked()?;
        let point = screen_point(x, y)?;
        check_reach(unsafe { GetForegroundWindow() })?;
        let mut target = unsafe { WindowFromPoint(point) };
        if target != 0 {
            let root = unsafe { GetAncestor(target, 2) };
            if root != 0 {
                target = root;
            }
        }
        check_reach(target)?;
        self.move_pointer(x, y)?;
        let (down, up) = match button {
            "right" => (0x0008, 0x0010),
            "middle" => (0x0020, 0x0040),
            _ => (0x0002, 0x0004),
        };
        for _ in 0..if double { 2 } else { 1 } {
            send_inputs(&[mouse_input(down), mouse_input(up)])?;
        }
        Ok(())
    }
    fn type_text(&self, text: &str) -> Result<()> {
        require_unlocked()?;
        let foreground = unsafe { GetForegroundWindow() };
        check_reach(foreground)?;
        let thread = unsafe { GetWindowThreadProcessId(foreground, ptr::null_mut()) };
        let layout = unsafe { GetKeyboardLayout(thread) };
        let caps_on = unsafe { GetKeyState(0x14) } & 1 != 0;
        let groups = plan_text(
            text,
            |ch| {
                if ch as u32 > u16::MAX as u32 {
                    return None;
                }
                let mapped = unsafe { VkKeyScanExW(ch as u16, layout) };
                if mapped == -1 {
                    return None;
                }
                let state = (mapped >> 8) as u8;
                if state & !1 != 0 {
                    return None;
                }
                let vk = mapped as u16 & 0xff;
                let mut shift = state & 1 != 0;
                if caps_on && (b'A' as u16..=b'Z' as u16).contains(&vk) {
                    shift = !shift;
                }
                Some((vk, shift))
            },
            |vk| unsafe { MapVirtualKeyExW(u32::from(vk), 0, layout) as u16 },
        );
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                thread::sleep(Duration::from_millis(5));
            }
            let inputs = group
                .iter()
                .map(|key| key_input(key.vk, key.scan, key.flags))
                .collect::<Vec<_>>();
            send_inputs(&inputs)?;
        }
        Ok(())
    }
    fn key(&self, chord: Chord) -> Result<()> {
        require_unlocked()?;
        check_reach(unsafe { GetForegroundWindow() })?;
        let mut events = Vec::new();
        for modifier in &chord.modifiers {
            events.push(vk_events(*modifier, false));
        }
        events.push(vk_events(chord.key, false));
        events.push(vk_events(chord.key, true));
        for modifier in chord.modifiers.iter().rev() {
            events.push(vk_events(*modifier, true));
        }
        send_inputs(&events)
    }
    fn run(&self, program: &str, args: &[String]) -> Result<u32> {
        require_unlocked()?;
        Command::new(program)
            .args(args)
            .creation_flags(0x10 | 0x200)
            .spawn()
            .map(|child| child.id())
            .map_err(|e| Error::internal(format!("start {program}: {e}")))
    }
    fn lock(&self) -> Result<()> {
        if unsafe { LockWorkStation() } == 0 {
            Err(win_error("LockWorkStation"))
        } else {
            Ok(())
        }
    }
}
