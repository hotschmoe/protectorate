//! Interactive terminal attach over the hub's framed WebSocket protocol.
use crate::{hub_url, token, Client, Code, Invocation, Result, RpcError};
use std::{
    io::{BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    sync::Arc,
};
use tungstenite::{client::IntoClientRequest, http::HeaderValue, stream::MaybeTlsStream, Message};

fn private_ca_pem() -> Result<Option<Vec<u8>>> {
    private_ca_pem_from(
        std::env::var("PROTECTORATE_TLS_CA_PEM").ok(),
        std::env::var_os("PROTECTORATE_TLS_CA_CERT"),
    )
}

fn private_ca_pem_from(
    pem: Option<String>,
    ca: Option<std::ffi::OsString>,
) -> Result<Option<Vec<u8>>> {
    if let Some(pem) = pem {
        if pem.len() > 16 * 1024 {
            return Err(RpcError::invalid("TLS CA environment exceeds 16 KiB"));
        }
        return Ok(Some(pem.into_bytes()));
    }
    let Some(path) = ca else {
        return Ok(None);
    };
    let path = Path::new(&path);
    let metadata = std::fs::metadata(path)
        .map_err(|e| RpcError::invalid(format!("TLS CA certificate: {e}")))?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(RpcError::invalid(
            "TLS CA must be a regular file under 1 MiB",
        ));
    }
    let mut pem = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| RpcError::invalid(format!("TLS CA certificate: {e}")))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut pem)
        .map_err(|e| RpcError::invalid(format!("TLS CA certificate: {e}")))?;
    if pem.len() > 1024 * 1024 {
        return Err(RpcError::invalid("TLS CA exceeds 1 MiB"));
    }
    Ok(Some(pem))
}

fn private_ca_config(pem: &[u8]) -> Result<Arc<rustls::ClientConfig>> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    if pem.is_empty() || pem.len() > 1024 * 1024 {
        return Err(RpcError::invalid("TLS CA bundle must be under 1 MiB"));
    }
    let mut reader = BufReader::new(pem);
    let mut roots = rustls::RootCertStore::empty();
    while let Some(item) = rustls_pemfile::read_one(&mut reader)
        .map_err(|e| RpcError::invalid(format!("invalid TLS CA PEM: {e}")))?
    {
        match item {
            rustls_pemfile::Item::X509Certificate(cert) => roots
                .add(cert)
                .map_err(|e| RpcError::invalid(format!("invalid TLS CA certificate: {e}")))?,
            _ => {
                return Err(RpcError::invalid(
                    "TLS CA bundle must contain certificates only",
                ))
            }
        }
    }
    if roots.is_empty() {
        return Err(RpcError::invalid("TLS CA bundle has no certificates"));
    }
    Ok(Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

type Socket = tungstenite::WebSocket<MaybeTlsStream<TcpStream>>;

/// What can go wrong before a terminal frame is ever exchanged: a bad local
/// CA bundle, or the hub refusing the upgrade.
#[derive(Debug)]
enum ConnectError {
    Config(RpcError),
    Socket(tungstenite::Error),
}

fn connect_websocket(
    request: tungstenite::http::Request<()>,
    pem: Option<&[u8]>,
) -> std::result::Result<Socket, ConnectError> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    if let Some(pem) = pem {
        let config = match private_ca_config(pem) {
            Ok(config) => config,
            Err(error) => return Err(ConnectError::Config(error)),
        };
        let uri = request.uri();
        let host = match uri.host() {
            Some(host) => host,
            None => {
                return Err(ConnectError::Config(RpcError::invalid(
                    "hub URL has no host",
                )))
            }
        };
        let port = uri
            .port_u16()
            .unwrap_or(if uri.scheme_str() == Some("wss") {
                443
            } else {
                80
            });
        let stream = match TcpStream::connect((host.trim_matches(['[', ']']), port)) {
            Ok(stream) => stream,
            Err(error) => {
                return Err(ConnectError::Config(RpcError::new(
                    Code::Unavailable,
                    format!("connect terminal: {error}"),
                )))
            }
        };
        tungstenite::client_tls_with_config(
            request,
            stream,
            None,
            Some(tungstenite::Connector::Rustls(config)),
        )
        .map(|(socket, _)| socket)
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => ConnectError::Socket(error),
            tungstenite::HandshakeError::Interrupted(_) => ConnectError::Config(RpcError::new(
                Code::Internal,
                "attach handshake interrupted",
            )),
        })
    } else {
        tungstenite::connect(request)
            .map(|(socket, _)| socket)
            .map_err(ConnectError::Socket)
    }
}

/// The hub refused the upgrade. Its own status says why: 401 for a bad token,
/// 403 for a token that may not host a terminal, 404 for a session that is
/// not there.
fn attach_error(name: &str, error: ConnectError) -> RpcError {
    match error {
        ConnectError::Config(error) => error,
        ConnectError::Socket(tungstenite::Error::Http(response)) => {
            let status = response.status();
            let code = match status.as_u16() {
                400 => Code::InvalidArgument,
                401 => Code::Unauthenticated,
                403 => Code::PermissionDenied,
                404 => Code::NotFound,
                409 => Code::AlreadyExists,
                429 => Code::ResourceExhausted,
                502..=504 => Code::Unavailable,
                _ => Code::Unknown,
            };
            RpcError::new(
                code,
                format!(
                    "attach {name}: hub refused the terminal: HTTP {}",
                    status.as_u16()
                ),
            )
        }
        ConnectError::Socket(error) => {
            RpcError::new(Code::Unavailable, format!("attach {name}: {error}"))
        }
    }
}

#[cfg(unix)]
struct RawTerminal(libc::termios);

#[cfg(unix)]
impl RawTerminal {
    fn enter() -> Result<Self> {
        if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1
            || unsafe { libc::isatty(libc::STDOUT_FILENO) } != 1
        {
            return Err(RpcError::invalid("attach needs an interactive terminal"));
        }
        let mut state = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, state.as_mut_ptr()) } < 0 {
            return Err(io_error("terminal state"));
        }
        let state = unsafe { state.assume_init() };
        let mut raw = state;
        unsafe {
            libc::cfmakeraw(&raw mut raw);
        }
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } < 0 {
            return Err(io_error("raw terminal"));
        }
        Ok(Self(state))
    }

    fn poll_input(&self) -> Result<Option<Vec<u8>>> {
        let mut poll = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&raw mut poll, 1, 0) } <= 0 || poll.revents & libc::POLLIN == 0 {
            return Ok(None);
        }
        let mut bytes = vec![0_u8; 4096];
        let count =
            unsafe { libc::read(libc::STDIN_FILENO, bytes.as_mut_ptr().cast(), bytes.len()) };
        if count < 0 {
            return Err(io_error("terminal input"));
        }
        bytes.truncate(count as usize);
        Ok(Some(bytes))
    }
}

#[cfg(unix)]
impl Drop for RawTerminal {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.0);
        }
    }
}

#[cfg(unix)]
fn io_error(label: &str) -> RpcError {
    RpcError::new(
        Code::Internal,
        format!("{label}: {}", std::io::Error::last_os_error()),
    )
}

#[cfg(unix)]
fn size() -> (u16, u16) {
    let mut dimensions = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    if unsafe {
        libc::ioctl(
            libc::STDOUT_FILENO,
            libc::TIOCGWINSZ,
            dimensions.as_mut_ptr(),
        )
    } < 0
    {
        return (80, 24);
    }
    let dimensions = unsafe { dimensions.assume_init() };
    (dimensions.ws_col.max(1), dimensions.ws_row.max(1))
}

#[cfg(windows)]
struct RawTerminal {
    input: windows_sys::Win32::Foundation::HANDLE,
    output: windows_sys::Win32::Foundation::HANDLE,
    input_mode: u32,
    output_mode: u32,
    input_cp: u32,
    output_cp: u32,
    incoming: Option<std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CancelSynchronousIo(thread: windows_sys::Win32::Foundation::HANDLE) -> i32;
}

#[cfg(windows)]
impl RawTerminal {
    fn enter() -> Result<Self> {
        use windows_sys::Win32::System::Console::{
            GetConsoleCP, GetConsoleMode, GetConsoleOutputCP, GetStdHandle, SetConsoleCP,
            SetConsoleMode, SetConsoleOutputCP, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
            ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT,
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        let mut input_mode = 0;
        let mut output_mode = 0;
        if input.is_null()
            || output.is_null()
            || unsafe { GetConsoleMode(input, &mut input_mode) } == 0
            || unsafe { GetConsoleMode(output, &mut output_mode) } == 0
        {
            return Err(RpcError::invalid("attach needs an interactive terminal"));
        }
        let input_cp = unsafe { GetConsoleCP() };
        let output_cp = unsafe { GetConsoleOutputCP() };
        let raw_input = (input_mode
            & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        if unsafe { SetConsoleMode(input, raw_input) } == 0 {
            return Err(win_error("raw terminal input"));
        }
        if unsafe { SetConsoleMode(output, output_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) } == 0
        {
            unsafe { SetConsoleMode(input, input_mode) };
            return Err(win_error("virtual terminal output"));
        }
        if unsafe { SetConsoleCP(65001) } == 0 || unsafe { SetConsoleOutputCP(65001) } == 0 {
            unsafe {
                SetConsoleMode(input, input_mode);
                SetConsoleMode(output, output_mode);
                SetConsoleCP(input_cp);
                SetConsoleOutputCP(output_cp);
            }
            return Err(win_error("UTF-8 terminal"));
        }
        let (send, receive) = std::sync::mpsc::sync_channel(8);
        let handle = input as usize;
        let reader = std::thread::spawn(move || {
            use windows_sys::Win32::Storage::FileSystem::ReadFile;
            loop {
                let mut bytes = vec![0_u8; 4096];
                let mut count = 0;
                let ok = unsafe {
                    ReadFile(
                        handle as windows_sys::Win32::Foundation::HANDLE,
                        bytes.as_mut_ptr().cast(),
                        bytes.len() as u32,
                        &mut count,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    let _ = send.send(Err(std::io::Error::last_os_error()));
                    break;
                }
                bytes.truncate(count as usize);
                let eof = bytes.is_empty();
                if send.send(Ok(bytes)).is_err() || eof {
                    break;
                }
            }
        });
        Ok(Self {
            input,
            output,
            input_mode,
            output_mode,
            input_cp,
            output_cp,
            incoming: Some(receive),
            reader: Some(reader),
        })
    }

    fn poll_input(&self) -> Result<Option<Vec<u8>>> {
        use std::sync::mpsc::TryRecvError;
        match self
            .incoming
            .as_ref()
            .expect("terminal input receiver")
            .try_recv()
        {
            Ok(Ok(bytes)) => Ok(Some(bytes)),
            Ok(Err(error)) => Err(RpcError::new(
                Code::Unavailable,
                format!("terminal input: {error}"),
            )),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Ok(Some(Vec::new())),
        }
    }
}

#[cfg(windows)]
impl Drop for RawTerminal {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Console::{
            SetConsoleCP, SetConsoleMode, SetConsoleOutputCP,
        };
        self.incoming.take();
        if let Some(reader) = self.reader.take() {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
            while !reader.is_finished() && std::time::Instant::now() < deadline {
                unsafe { CancelSynchronousIo(reader.as_raw_handle().cast()) };
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if reader.is_finished() {
                let _ = reader.join();
            }
        }
        unsafe {
            SetConsoleMode(self.input, self.input_mode);
            SetConsoleMode(self.output, self.output_mode);
            SetConsoleCP(self.input_cp);
            SetConsoleOutputCP(self.output_cp);
        }
    }
}

#[cfg(windows)]
fn win_error(label: &str) -> RpcError {
    RpcError::new(
        Code::Internal,
        format!("{label}: {}", std::io::Error::last_os_error()),
    )
}

#[cfg(windows)]
fn size() -> (u16, u16) {
    use windows_sys::Win32::System::Console::{
        GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
    };
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    if output.is_null() || unsafe { GetConsoleScreenBufferInfo(output, &mut info) } == 0 {
        return (80, 24);
    }
    let window = info.srWindow;
    (
        (window.Right - window.Left + 1).max(1) as u16,
        (window.Bottom - window.Top + 1).max(1) as u16,
    )
}

fn url(base: &str, session: &str, read_only: bool, cols: u16, rows: u16) -> Result<String> {
    let base = if let Some(url) = base.strip_prefix("https://") {
        format!("wss://{url}")
    } else if let Some(url) = base.strip_prefix("http://") {
        format!("ws://{url}")
    } else {
        return Err(RpcError::invalid("hub URL must use HTTP or HTTPS"));
    };
    let readonly = if read_only { "&readonly=1" } else { "" };
    Ok(format!(
        "{}/ws/terminal/{}?cols={cols}&rows={rows}{readonly}",
        base.trim_end_matches('/'),
        encode_segment(session)
    ))
}

/// Percent-encode one URL path segment. Hub session names are already 1-64 of
/// A-Z a-z 0-9 _ -, so this only ever changes anything for a name the hub
/// would reject anyway: it keeps a raw name from escaping the path it is put
/// into.
fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(unix)]
fn require_tty() -> Result<()> {
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1
        || unsafe { libc::isatty(libc::STDOUT_FILENO) } != 1
    {
        return Err(RpcError::invalid("attach needs an interactive terminal"));
    }
    Ok(())
}

#[cfg(windows)]
fn require_tty() -> Result<()> {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let mut mode = 0;
    if input.is_null()
        || output.is_null()
        || unsafe { GetConsoleMode(input, &mut mode) } == 0
        || unsafe { GetConsoleMode(output, &mut mode) } == 0
    {
        return Err(RpcError::invalid("attach needs an interactive terminal"));
    }
    Ok(())
}

/// `sleeve attach`: the sleeve record says which session to open.
#[cfg(any(unix, windows))]
pub(super) fn run(
    client: &Client,
    invocation: &Invocation,
    name: &str,
    read_only: bool,
    escape: &str,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<String> {
    require_tty()?;
    let response = client.call(
        "/protectorate.v1.SleeveService/GetSleeve",
        &serde_json::json!({"name":name}),
    )?;
    let session = response["sleeve"]["session"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RpcError::new(Code::DataLoss, "sleeve has no session"))?;
    bridge(invocation, name, session, read_only, escape, stdout, stderr)
}

/// `session attach`: the name is the session itself, no sleeve record.
#[cfg(any(unix, windows))]
pub(super) fn run_session(
    invocation: &Invocation,
    name: &str,
    read_only: bool,
    escape: &str,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<String> {
    require_tty()?;
    bridge(invocation, name, name, read_only, escape, stdout, stderr)
}

#[cfg(any(unix, windows))]
fn bridge(
    invocation: &Invocation,
    name: &str,
    session: &str,
    read_only: bool,
    escape: &str,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<String> {
    let (mut cols, mut rows) = size();
    let url = url(&hub_url(invocation), session, read_only, cols, rows)?;
    let mut request = url
        .into_client_request()
        .map_err(|e| RpcError::invalid(e.to_string()))?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", token(invocation)?))
            .map_err(|e| RpcError::invalid(e.to_string()))?,
    );
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static("protectorate.v1"),
    );
    let ca_pem = private_ca_pem()?;
    let mut socket =
        connect_websocket(request, ca_pem.as_deref()).map_err(|error| attach_error(name, error))?;
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            stream.set_read_timeout(Some(std::time::Duration::from_millis(50)))
        }
        MaybeTlsStream::Rustls(stream) => stream
            .sock
            .set_read_timeout(Some(std::time::Duration::from_millis(50))),
        _ => {
            return Err(RpcError::new(
                Code::Unimplemented,
                "unsupported WebSocket transport",
            ))
        }
    }
    .map_err(|e| RpcError::new(Code::Internal, format!("attach socket timeout: {e}")))?;
    let raw = RawTerminal::enter()?;
    let escape_byte = if escape == "none" {
        None
    } else if escape == "^?" {
        Some(127)
    } else {
        Some(escape.as_bytes()[1] & 0x1f)
    };
    let mut notice = String::new();
    let mut detached = false;
    loop {
        if let Some(input) = raw.poll_input()? {
            if input.is_empty() {
                detached = true;
                break;
            }
            {
                let data = input.as_slice();
                let end =
                    escape_byte.and_then(|escape| data.iter().position(|byte| *byte == escape));
                let payload = &data[..end.unwrap_or(data.len())];
                if !read_only && !payload.is_empty() {
                    let mut frame = Vec::with_capacity(payload.len() + 1);
                    frame.push(b'0');
                    frame.extend_from_slice(payload);
                    socket
                        .send(Message::Binary(frame.into()))
                        .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))?;
                }
                if end.is_some() {
                    detached = true;
                    break;
                }
            }
        }
        let next_size = size();
        if next_size != (cols, rows) {
            (cols, rows) = next_size;
            let mut frame = vec![b'1'];
            frame.extend_from_slice(format!("{{\"cols\":{cols},\"rows\":{rows}}}").as_bytes());
            socket
                .send(Message::Binary(frame.into()))
                .map_err(|e| RpcError::new(Code::Unavailable, e.to_string()))?;
        }
        match socket.read() {
            Ok(Message::Binary(frame)) if frame.len() > 1 => match frame[0] {
                b'0' => {
                    stdout
                        .write_all(&frame[1..])
                        .and_then(|_| stdout.flush())
                        .map_err(|e| RpcError::new(Code::Internal, e.to_string()))?;
                }
                b'2' => notice = String::from_utf8_lossy(&frame[1..]).into_owned(),
                _ => {}
            },
            Ok(Message::Close(_)) => break,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(tungstenite::Error::ConnectionClosed) => break,
            Err(error) => {
                return Err(RpcError::new(
                    Code::Unavailable,
                    format!("attach {name}: {error}"),
                ))
            }
            _ => {}
        }
    }
    let _ = socket.close(None);
    let _ = stdout.write_all(
        b"\x1b[?1049l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?2004l\x1b[?25h\x1b[0m",
    );
    drop(raw);
    if !notice.is_empty() {
        let _ = writeln!(stderr, "[{notice}]");
    }
    let status = if detached {
        "detached from"
    } else {
        "connection to"
    };
    let _ = writeln!(
        stderr,
        "[{status} {name}{}]",
        if detached { "" } else { " closed" }
    );
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;

    #[tokio::test]
    async fn private_ca_wss_accepts_only_the_pinned_trust_bundle() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let generated = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let wrong = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let pem = generated.cert.pem();
        let directory = tempfile::tempdir().unwrap();
        let ca_path = directory.path().join("private-ca.pem");
        std::fs::write(&ca_path, &pem).unwrap();
        let loaded_ca = private_ca_pem_from(None, Some(ca_path.into_os_string()))
            .unwrap()
            .unwrap();
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![generated.cert.der().clone()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(
                    rustls::pki_types::PrivatePkcs8KeyDer::from(
                        generated.signing_key.serialize_der(),
                    ),
                ),
            )
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
            for _ in 0..2 {
                let (tcp, _) = listener.accept().await.unwrap();
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let mut socket = tokio_tungstenite::accept_async(tls).await.unwrap();
                    socket
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            vec![b'0', b'o', b'k'].into(),
                        ))
                        .await
                        .unwrap();
                    let _ = socket.close(None).await;
                }
            }
        });
        let url = format!("wss://{address}/ws/terminal/sleeve-test");
        let result = tokio::task::spawn_blocking(move || {
            let request = url.clone().into_client_request().unwrap();
            let denied = connect_websocket(request, Some(wrong.cert.pem().as_bytes()));
            assert!(denied.is_err());
            let request = url.into_client_request().unwrap();
            let mut accepted = connect_websocket(request, Some(&loaded_ca)).unwrap();
            assert_eq!(accepted.read().unwrap().into_data().as_ref(), b"0ok");
            let _ = accepted.close(None);
        })
        .await;
        result.unwrap();
        server.await.unwrap();
    }

    #[test]
    fn private_ca_rejects_non_certificate_pem() {
        assert!(private_ca_config(b"").is_err());
        assert!(private_ca_config(
            b"-----BEGIN PRIVATE KEY-----\nAA==\n-----END PRIVATE KEY-----\n"
        )
        .is_err());
        let directory = tempfile::tempdir().unwrap();
        let ca_path = directory.path().join("ca.pem");
        std::fs::write(&ca_path, b"file-ca").unwrap();
        assert_eq!(
            private_ca_pem_from(
                Some("env-ca".into()),
                Some(ca_path.clone().into_os_string())
            )
            .unwrap(),
            Some(b"env-ca".to_vec()),
        );
        assert_eq!(
            private_ca_pem_from(None, Some(ca_path.clone().into_os_string())).unwrap(),
            Some(b"file-ca".to_vec()),
        );
        std::fs::write(&ca_path, vec![b'x'; 1024 * 1024 + 1]).unwrap();
        assert!(private_ca_pem_from(None, Some(ca_path.into_os_string())).is_err());
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn run(
    _client: &Client,
    _invocation: &Invocation,
    _name: &str,
    _read_only: bool,
    _escape: &str,
    _stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<String> {
    Err(RpcError::new(
        Code::Unimplemented,
        "terminal attach is not yet supported on this platform",
    ))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn run_session(
    _invocation: &Invocation,
    _name: &str,
    _read_only: bool,
    _escape: &str,
    _stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> Result<String> {
    Err(RpcError::new(
        Code::Unimplemented,
        "terminal attach is not yet supported on this platform",
    ))
}
