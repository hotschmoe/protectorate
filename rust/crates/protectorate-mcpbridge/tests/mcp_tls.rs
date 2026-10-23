use protectorate_mcpbridge::tls_config;
#[cfg(any(unix, windows))]
use protectorate_mcpbridge::{bind_tls, serve_tls, Bridge, Config, HttpConfig};
use std::fs;
#[cfg(any(unix, windows))]
use std::{io::BufReader, sync::Arc, time::Duration};
#[cfg(any(unix, windows))]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
#[cfg(any(unix, windows))]
use tokio_rustls::{rustls, TlsConnector};

#[cfg(windows)]
fn make_windows_private(path: &std::path::Path) {
    use std::{
        ffi::OsStr,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER},
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
        },
        Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE, WRITE_DAC},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut raw_token = std::ptr::null_mut();
        assert_ne!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token),
            0
        );
        let mut size = 0;
        GetTokenInformation(raw_token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(ERROR_INSUFFICIENT_BUFFER as i32)
        );
        let mut bytes = vec![0u8; size as usize];
        assert_ne!(
            GetTokenInformation(
                raw_token,
                TokenUser,
                bytes.as_mut_ptr().cast(),
                size,
                &mut size
            ),
            0
        );
        CloseHandle(raw_token);
        let mut sid = std::ptr::null_mut();
        assert_ne!(
            ConvertSidToStringSidW((*(bytes.as_ptr().cast::<TOKEN_USER>())).User.Sid, &mut sid),
            0
        );
        let mut len = 0;
        while *sid.add(len) != 0 {
            len += 1;
        }
        let sid_text = String::from_utf16(std::slice::from_raw_parts(sid, len)).unwrap();
        LocalFree(sid.cast());
        let sddl = format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{sid_text})");
        let wide: Vec<u16> = OsStr::new(&sddl).encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        assert_ne!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut()
            ),
            0
        );
        let mut present = 0;
        let mut acl = std::ptr::null_mut();
        let mut defaulted = 0;
        assert_ne!(
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted),
            0
        );
        assert_ne!(present, 0);
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC)
            .open(path)
            .unwrap();
        let status = SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl,
            std::ptr::null_mut(),
        );
        LocalFree(descriptor);
        assert_eq!(status, 0);
    }
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn https_requires_tls_and_bearer() {
    let dir = tempfile::tempdir().unwrap();
    let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    fs::write(&cert_path, generated.cert.pem()).unwrap();
    fs::write(&key_path, generated.signing_key.serialize_pem()).unwrap();
    #[cfg(windows)]
    make_windows_private(&key_path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let tls = tls_config(&cert_path, &key_path).unwrap();
    let listener = bind_tls("127.0.0.1:0", tls).await.unwrap();
    let addr = listener.local_addr();
    let bridge = Arc::new(Bridge::new(Config::without_hub("test")).unwrap());
    let task = tokio::spawn(serve_tls(
        listener,
        bridge,
        HttpConfig::new("secret").unwrap(),
        async {
            std::future::pending::<()>().await;
        },
    ));

    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#;
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(
            rustls_pemfile::certs(&mut BufReader::new(fs::File::open(&cert_path).unwrap()))
                .next()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
    let client = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    for (authorization, expected) in [("", "401"), ("Authorization: Bearer secret\r\n", "200")] {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let mut stream = TlsConnector::from(Arc::new(client.clone()))
            .connect(
                rustls::pki_types::ServerName::IpAddress(addr.ip().into()),
                tcp,
            )
            .await
            .unwrap();
        let request = format!("POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n{authorization}Content-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(
            response.starts_with(&format!("HTTP/1.1 {expected}")),
            "{response}"
        );
        if expected == "200" {
            assert!(response.contains("\"serverInfo\""), "{response}");
        }
    }

    let mut plain = TcpStream::connect(addr).await.unwrap();
    plain
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut response = [0_u8; 16];
    let read = tokio::time::timeout(Duration::from_secs(3), plain.read(&mut response))
        .await
        .unwrap();
    if let Ok(size) = read {
        assert!(
            !response[..size].starts_with(b"HTTP/"),
            "plaintext received HTTP response"
        );
    }
    task.abort();
}

#[test]
#[cfg(unix)]
fn rejects_public_or_linked_key() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");
    fs::write(&cert, generated.cert.pem()).unwrap();
    fs::write(&key, generated.signing_key.serialize_pem()).unwrap();
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(tls_config(&cert, &key)
        .unwrap_err()
        .message
        .contains("owner"));
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    let link = dir.path().join("key-link.pem");
    std::os::unix::fs::symlink(&key, &link).unwrap();
    assert!(tls_config(&cert, &link).is_err());
}
