//! Open TLS material by handle and reject redirected or broadly readable keys.
use std::{
    ffi::c_void,
    fs::{self, File, OpenOptions},
    io,
    os::windows::{fs::MetadataExt, io::AsRawHandle},
    path::{Component, Path},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE},
    Security::{
        Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT},
        GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_GENERIC_READ, READ_CONTROL},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

fn denied(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason)
}

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

unsafe fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _text = Local(text.cast());
    let mut len = 0;
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(text, len) })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid account SID"))
}

fn current_sid() -> io::Result<String> {
    unsafe {
        let mut handle = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Token(handle);
        let mut size = 0;
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut size);
        if io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || size < std::mem::size_of::<TOKEN_USER>() as u32
        {
            return Err(io::Error::last_os_error());
        }
        let mut bytes = vec![0u8; size as usize];
        if GetTokenInformation(
            token.0,
            TokenUser,
            bytes.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        sid_string((*(bytes.as_ptr().cast::<TOKEN_USER>())).User.Sid)
    }
}

fn check_key_acl(file: &File) -> io::Result<()> {
    let current = current_sid()?;
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
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let _descriptor = Local(descriptor);
        if owner.is_null() {
            return Err(denied("TLS key has no owner"));
        }
        let owner = sid_string(owner)?;
        if owner != current && owner != "S-1-5-18" && owner != "S-1-5-32-544" {
            return Err(denied("TLS key is owned by another principal"));
        }
        if acl.is_null() {
            return Err(denied("TLS key has a null DACL"));
        }
        let mut user_allowed = false;
        for index in 0..(*acl).AceCount as u32 {
            let mut ace = std::ptr::null_mut();
            if GetAce(acl, index, &mut ace) == 0 {
                return Err(io::Error::last_os_error());
            }
            let header = &*(ace.cast::<ACE_HEADER>());
            if header.AceType == 1 {
                continue; // An access-denied ACE cannot add readers.
            }
            if header.AceType != 0
                || header.AceSize < std::mem::size_of::<ACCESS_ALLOWED_ACE>() as u16
            {
                return Err(denied("TLS key has an unsupported access control entry"));
            }
            let allowed = &*(ace.cast::<ACCESS_ALLOWED_ACE>());
            let sid = (&allowed.SidStart as *const u32).cast_mut().cast();
            let sid = sid_string(sid)?;
            if sid == current {
                user_allowed = true;
            } else if sid != "S-1-5-18" && sid != "S-1-5-32-544" {
                return Err(denied("TLS key allows another principal"));
            }
        }
        if !user_allowed {
            return Err(denied("TLS key does not allow the current user"));
        }
    }
    Ok(())
}

pub(super) fn open_checked(path: &Path, key: bool) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    if path.components().any(|part| part == Component::ParentDir) {
        return Err(denied("TLS paths cannot contain parent traversal"));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    for ancestor in absolute.ancestors() {
        if fs::symlink_metadata(ancestor)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(denied("TLS path contains a reparse point"));
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ | READ_CONTROL)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(denied("TLS file is a reparse point"));
    }
    if key {
        check_key_acl(&file)?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsStr,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    };
    use windows_sys::Win32::Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo, SDDL_REVISION_1,
        },
        GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_WRITE, WRITE_DAC};

    fn acl_writer(path: &Path) -> File {
        OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
            .open(path)
            .unwrap()
    }

    fn set_acl(file: &File, sddl: &str) {
        let wide: Vec<u16> = OsStr::new(sddl).encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        let _descriptor = Local(descriptor);
        let mut present = 0;
        let mut acl = std::ptr::null_mut();
        let mut defaulted = 0;
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted)
            },
            0
        );
        assert_ne!(present, 0);
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
        assert_eq!(status, 0, "{}", io::Error::from_raw_os_error(status as i32));
    }

    #[test]
    fn accepts_private_key_and_rejects_broad_access() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.pem");
        fs::write(&path, b"secret").unwrap();
        let file = acl_writer(&path);
        let current = current_sid().unwrap();
        set_acl(
            &file,
            &format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{current})"),
        );
        drop(file);
        open_checked(&path, true).unwrap();

        let file = acl_writer(&path);
        set_acl(&file, &format!("D:P(A;;FA;;;{current})(A;;FR;;;WD)"));
        drop(file);
        let error = open_checked(&path, true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("another principal"));
        // A public certificate may use the same ACL.
        open_checked(&path, false).unwrap();
    }

    #[test]
    fn rejects_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("..").join("key.pem");
        let error = open_checked(&path, true).unwrap_err();
        assert!(error.to_string().contains("parent traversal"));
    }

    #[test]
    fn rejects_reparse_key_and_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.pem");
        fs::write(&path, b"secret").unwrap();
        let link = dir.path().join("key-link.pem");
        std::os::windows::fs::symlink_file(&path, &link).unwrap();
        let error = open_checked(&link, true).unwrap_err();
        assert!(error.to_string().contains("reparse point"));

        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("key.pem"), b"secret").unwrap();
        let parent_link = dir.path().join("sub-link");
        std::os::windows::fs::symlink_dir(&sub, &parent_link).unwrap();
        let error = open_checked(&parent_link.join("key.pem"), true).unwrap_err();
        assert!(error.to_string().contains("reparse point"));
    }
}
