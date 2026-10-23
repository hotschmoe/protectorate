//! Windows credential storage. A protected DACL is installed at creation, so
//! the temporary file never contains a secret under an inherited public ACL.
use protectorate_wire::{Code, Result, RpcError};
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{ffi::OsStrExt, fs::MetadataExt, io::AsRawHandle},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL, WRITE_DAC,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn denial(path: &Path, reason: &str) -> RpcError {
    RpcError::new(
        Code::PermissionDenied,
        format!("{}: {reason}; use a private state home", path.display()),
    )
}

fn internal(action: &str, path: &Path, error: impl std::fmt::Display) -> RpcError {
    RpcError::new(
        Code::Internal,
        format!("{action} {}: {error}", path.display()),
    )
}

struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

fn current_sid() -> io::Result<String> {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        struct Token(windows_sys::Win32::Foundation::HANDLE);
        impl Drop for Token {
            fn drop(&mut self) {
                unsafe { CloseHandle(self.0) };
            }
        }
        let token = Token(token);
        let mut size = 0;
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut size);
        if io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || size < std::mem::size_of::<TOKEN_USER>() as u32
        {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0u8; size as usize];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let sid = (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        sid_string(sid)
    }
}

unsafe fn sid_string(sid: PSID) -> io::Result<String> {
    let mut out = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut out) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let local = Local(out.cast());
    let mut len = 0;
    while unsafe { *out.add(len) } != 0 {
        len += 1;
    }
    let string = String::from_utf16(unsafe { std::slice::from_raw_parts(out, len) })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid SID text"));
    drop(local);
    string
}

fn private_descriptor(directory: bool) -> io::Result<Local> {
    let sid = current_sid()?;
    let inherit = if directory { "OICI" } else { "" };
    let sddl = wide(OsStr::new(&format!(
        "D:P(A;{inherit};FA;;;SY)(A;{inherit};FA;;;BA)(A;{inherit};FA;;;{sid})"
    )));
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(Local(descriptor))
}

fn is_reparse(path: &Path) -> io::Result<bool> {
    Ok(fs::symlink_metadata(path)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Existing components may not redirect the credential path. The private
/// state home and credentials directory are checked separately for their ACL.
fn check_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match is_reparse(ancestor) {
            Ok(true) => return Err(denial(ancestor, "reparse points are refused")),
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(internal("inspect", ancestor, error)),
        }
    }
    Ok(())
}

/// Every allow ACE on a private object must belong to the user, SYSTEM, or
/// Administrators. Unknown ACE forms fail closed. Inherited broad access is
/// rejected as well.
fn check_acl(file: &File, path: &Path) -> Result<()> {
    let current = current_sid().map_err(|error| internal("identify user for", path, error))?;
    unsafe {
        let mut acl: *mut ACL = std::ptr::null_mut();
        let mut owner: PSID = std::ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let status = GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        );
        if status != 0 {
            return Err(internal(
                "inspect ACL of",
                path,
                io::Error::from_raw_os_error(status as i32),
            ));
        }
        let _descriptor = Local(descriptor);
        if owner.is_null() {
            return Err(denial(path, "object has no owner"));
        }
        let owner = sid_string(owner).map_err(|error| internal("inspect owner of", path, error))?;
        if owner != current && owner != "S-1-5-18" && owner != "S-1-5-32-544" {
            return Err(denial(path, "object is owned by another principal"));
        }
        if acl.is_null() {
            return Err(denial(path, "a null DACL grants access to everyone"));
        }
        let mut owner_allowed = false;
        for index in 0..(*acl).AceCount as u32 {
            let mut ace = std::ptr::null_mut();
            if GetAce(acl, index, &mut ace) == 0 {
                return Err(internal("inspect ACL of", path, io::Error::last_os_error()));
            }
            let header = &*(ace.cast::<windows_sys::Win32::Security::ACE_HEADER>());
            if header.AceType == 1 {
                continue; // deny ACEs only narrow access
            }
            if header.AceType != 0
                || header.AceSize < std::mem::size_of::<ACCESS_ALLOWED_ACE>() as u16
            {
                return Err(denial(path, "unsupported access control entry"));
            }
            let allowed = &*(ace.cast::<ACCESS_ALLOWED_ACE>());
            let sid = (&allowed.SidStart as *const u32).cast_mut().cast();
            let sid = sid_string(sid).map_err(|error| internal("inspect ACL of", path, error))?;
            if sid == current {
                owner_allowed = true;
            } else if sid != "S-1-5-18" && sid != "S-1-5-32-544" {
                return Err(denial(path, "another principal has access"));
            }
        }
        if !owner_allowed {
            return Err(denial(path, "current user has no explicit access"));
        }
    }
    Ok(())
}

fn inspect_private(path: &Path, directory: bool) -> Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    check_ancestors(path)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(READ_CONTROL | FILE_GENERIC_READ)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    if directory {
        options.custom_flags(0x0200_0000 | 0x0020_0000); // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
    } else {
        options.custom_flags(0x0020_0000);
    }
    let file = options
        .open(path)
        .map_err(|error| internal("open", path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| internal("inspect", path, error))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.is_dir() != directory
    {
        return Err(denial(path, "unexpected file type or reparse point"));
    }
    check_acl(&file, path)?;
    Ok(file)
}

pub(super) fn check_private_directory(dir: &Path) -> Result<(File, File)> {
    let home = dir
        .parent()
        .ok_or_else(|| denial(dir, "missing state home"))?;
    let home = inspect_private(home, true)?;
    let credentials = inspect_private(dir, true)?;
    Ok((home, credentials))
}

pub(super) fn ensure_private_directory(dir: &Path) -> Result<()> {
    let home = dir
        .parent()
        .ok_or_else(|| denial(dir, "missing state home"))?;
    for path in [home, dir] {
        check_ancestors(path)?;
        if !path.exists() {
            let descriptor = private_descriptor(true)
                .map_err(|error| internal("prepare ACL for", path, error))?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: 0,
            };
            let result = unsafe { CreateDirectoryW(wide(path.as_os_str()).as_ptr(), &attributes) };
            if result == 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                return Err(internal(
                    "create private directory",
                    path,
                    io::Error::last_os_error(),
                ));
            }
        }
        inspect_private(path, true)?;
    }
    Ok(())
}

/// Harness auth trees have variable depth (`auth/<provider>` and
/// `login/claude`). Create each missing component with a protected DACL, then
/// verify the requested directory's owner, DACL, and real file type.
pub(super) fn ensure_private_path(path: &Path) -> Result<()> {
    check_ancestors(path)?;
    let mut missing = Vec::new();
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(ancestor),
            Err(error) => return Err(internal("inspect", ancestor, error)),
        }
    }
    for missing_path in missing.into_iter().rev() {
        let descriptor = private_descriptor(true)
            .map_err(|error| internal("prepare ACL for", missing_path, error))?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let result =
            unsafe { CreateDirectoryW(wide(missing_path.as_os_str()).as_ptr(), &attributes) };
        if result == 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
            return Err(internal(
                "create private directory",
                missing_path,
                io::Error::last_os_error(),
            ));
        }
        inspect_private(missing_path, true)?;
    }
    inspect_private(path, true)?;
    Ok(())
}

pub(super) fn check_private_file(path: &Path) -> Result<()> {
    inspect_private(path, false)?;
    Ok(())
}

pub(super) fn check_private_path(path: &Path) -> Result<()> {
    inspect_private(path, true)?;
    Ok(())
}

pub(super) fn read_private_file(path: &Path) -> io::Result<String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let _directory_guards = check_private_directory(dir)
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.message))?;
    let file = inspect_private(path, false)
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.message))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "credential exceeds 64 KiB",
        ));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(super) fn write_private_file(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let _directory_guards = check_private_directory(dir)
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.message))?;
    match fs::symlink_metadata(path) {
        Ok(_) => {
            inspect_private(path, false)
                .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.message))?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let descriptor = private_descriptor(false)?;
    let basename = path.file_name().unwrap_or_default().to_string_lossy();
    let (mut file, temp): (File, PathBuf) = loop {
        let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let temp = dir.join(format!(".{basename}.{}.{}", std::process::id(), n));
        // The parent has already passed an ACL check. CreateNew guarantees no
        // existing link is followed, then we set an explicit DACL before data.
        use std::os::windows::fs::OpenOptionsExt;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .access_mode(FILE_GENERIC_WRITE | WRITE_DAC)
            .open(&temp)
        {
            Ok(file) => break (file, temp),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        use windows_sys::Win32::{
            Security::Authorization::{SetSecurityInfo, SE_FILE_OBJECT},
            Security::{GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION},
        };
        let mut present = 0;
        let mut acl = std::ptr::null_mut();
        let mut defaulted = 0;
        if unsafe {
            GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
        } == 0
            || present == 0
        {
            return Err(io::Error::last_os_error());
        }
        let status = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
