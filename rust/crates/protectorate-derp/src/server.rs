use crate::cert::{write_private, Certificate};
use crate::hex;
use crate::protocol::{read_frames, take_frame, write_frame, Frame, MAX_PACKET};
use crypto_box::{
    aead::{Aead, AeadCore, OsRng},
    PublicKey, SalsaBox, SecretKey,
};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type PeerKey = [u8; 32];
type Peers = Arc<Mutex<HashMap<PeerKey, Arc<mpsc::SyncSender<Vec<u8>>>>>>;
const MAX_CLIENTS: usize = 256;

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: SocketAddr,
    pub stun_listen: SocketAddr,
    pub hostname: String,
    pub verify_url: String,
    pub state_dir: PathBuf,
}

pub struct Service {
    pub certificate_pin: String,
    pub derp_addr: SocketAddr,
    pub stun_addr: SocketAddr,
    stop: Arc<AtomicBool>,
    connections: Arc<Mutex<HashMap<usize, TcpStream>>>,
    clients: Arc<Mutex<Vec<JoinHandle<()>>>>,
    listener: Option<JoinHandle<()>>,
    stun: Option<JoinHandle<()>>,
}

impl Service {
    pub fn start(config: Config) -> io::Result<Self> {
        if !local_verify_url(&config.verify_url) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "verify_url must use local HTTP",
            ));
        }
        let cert = Certificate::load_or_create(&config.state_dir, &config.hostname)?;
        let key = load_or_create_key(&config.state_dir)?;
        let tls = Arc::new(
            ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
                .with_safe_default_protocol_versions()
                .map_err(io::Error::other)?
                .with_no_client_auth()
                .with_single_cert(
                    vec![cert.der.clone().into()],
                    rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_der.clone().into()),
                )
                .map_err(io::Error::other)?,
        );
        let tcp = TcpListener::bind(config.listen)?;
        let udp = UdpSocket::bind(config.stun_listen)?;
        let derp_addr = tcp.local_addr()?;
        let stun_addr = udp.local_addr()?;
        tcp.set_nonblocking(true)?;
        udp.set_read_timeout(Some(Duration::from_millis(150)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let next_client = Arc::new(AtomicUsize::new(0));
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let clients: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
        let peers: Peers = Arc::new(Mutex::new(HashMap::new()));
        let listener_stop = stop.clone();
        let listener_connections = connections.clone();
        let listener_clients = clients.clone();
        let listener = thread::spawn(move || {
            while !listener_stop.load(Ordering::Relaxed) {
                match tcp.accept() {
                    Ok((socket, remote)) => {
                        if active
                            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                                (n < MAX_CLIENTS).then_some(n + 1)
                            })
                            .is_err()
                        {
                            drop(socket);
                            continue;
                        }
                        let id = next_client.fetch_add(1, Ordering::Relaxed);
                        let Ok(shutdown_socket) = socket.try_clone() else {
                            active.fetch_sub(1, Ordering::AcqRel);
                            continue;
                        };
                        listener_connections
                            .lock()
                            .unwrap()
                            .insert(id, shutdown_socket);
                        let (tls, key, url, peers, stop) = (
                            tls.clone(),
                            key.clone(),
                            config.verify_url.clone(),
                            peers.clone(),
                            listener_stop.clone(),
                        );
                        let active = active.clone();
                        let connections = listener_connections.clone();
                        let handle = thread::spawn(move || {
                            let _slot = ClientSlot {
                                active,
                                connections,
                                id,
                            };
                            let _ = serve_client(socket, remote, tls, key, &url, peers, stop);
                        });
                        let mut clients = listener_clients.lock().unwrap();
                        let mut i = 0;
                        while i < clients.len() {
                            if clients[i].is_finished() {
                                let _ = clients.swap_remove(i).join();
                            } else {
                                i += 1;
                            }
                        }
                        clients.push(handle);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20))
                    }
                    Err(_) => break,
                }
            }
        });
        let stun_stop = stop.clone();
        let stun = thread::spawn(move || {
            let mut packet = [0u8; 2048];
            while !stun_stop.load(Ordering::Relaxed) {
                match udp.recv_from(&mut packet) {
                    Ok((n, addr)) => {
                        if let Some(response) = crate::stun::stun_response(&packet[..n], addr) {
                            let _ = udp.send_to(&response, addr);
                        }
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::TimedOut => {}
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            certificate_pin: cert.pin(),
            derp_addr,
            stun_addr,
            stop,
            connections,
            clients,
            listener: Some(listener),
            stun: Some(stun),
        })
    }
}

fn local_verify_url(url: &str) -> bool {
    let Some((authority, path)) = url.strip_prefix("http://").and_then(|s| s.split_once('/'))
    else {
        return false;
    };
    if path != "verify" {
        return false;
    }
    if let Ok(addr) = authority.parse::<SocketAddr>() {
        return addr.ip().is_loopback();
    }
    authority
        .strip_prefix("localhost:")
        .is_some_and(|port| port.parse::<u16>().is_ok())
}

struct ClientSlot {
    active: Arc<AtomicUsize>,
    connections: Arc<Mutex<HashMap<usize, TcpStream>>>,
    id: usize,
}

impl Drop for ClientSlot {
    fn drop(&mut self) {
        self.connections.lock().unwrap().remove(&self.id);
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.listener.take() {
            let _ = handle.join();
        }
        for socket in self.connections.lock().unwrap().values() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        for handle in self.clients.lock().unwrap().drain(..) {
            let _ = handle.join();
        }
        if let Some(handle) = self.stun.take() {
            let _ = handle.join();
        }
    }
}

fn load_or_create_key(dir: &std::path::Path) -> io::Result<SecretKey> {
    fs::create_dir_all(dir)?;
    let path = dir.join("derp_node_key");
    if path.exists() {
        let bytes = fs::read(path)?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad DERP key"))?;
        return Ok(SecretKey::from(key));
    }
    let key = SecretKey::generate(&mut OsRng);
    write_private(&path, &key.to_bytes())?;
    Ok(key)
}

fn serve_client(
    socket: TcpStream,
    remote: SocketAddr,
    tls: Arc<ServerConfig>,
    key: SecretKey,
    verify_url: &str,
    peers: Peers,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut stream = StreamOwned::new(
        ServerConnection::new(tls).map_err(io::Error::other)?,
        socket,
    );
    let request = read_request(&mut stream)?;
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("");
    if path == "/generate_204" {
        stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")?;
        return Ok(());
    }
    if path == "/derp/probe" || path == "/derp/latency-check" {
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        )?;
        return Ok(());
    }
    let upgrade = request
        .lines()
        .any(|line| line.eq_ignore_ascii_case("Upgrade: derp"));
    if path != "/derp" || !upgrade {
        stream.write_all(b"HTTP/1.1 426 Upgrade Required\r\nContent-Length: 0\r\n\r\n")?;
        return Ok(());
    }
    let fast = request
        .lines()
        .any(|line| line.eq_ignore_ascii_case("Derp-Fast-Start: 1"));
    if !fast {
        let response = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: DERP\r\nConnection: Upgrade\r\nDerp-Version: 2\r\nDerp-Public-Key: {}\r\n\r\n", hex(key.public_key().as_bytes()));
        stream.write_all(response.as_bytes())?;
    }
    let mut greeting = b"DERP\xf0\x9f\x94\x91".to_vec();
    greeting.extend_from_slice(key.public_key().as_bytes());
    write_frame(&mut stream, 0x01, &greeting)?;
    let mut pending = Vec::new();
    let mut buffer = [0u8; 8192];
    let client_key = loop {
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }
        pending.extend_from_slice(&buffer[..n]);
        let Some(frame) = take_frame(&mut pending, 256 * 1024)? else {
            continue;
        };
        if frame.kind != 0x02 || frame.payload.len() < 72 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad client info",
            ));
        }
        let client_key: PeerKey = frame.payload[..32].try_into().unwrap();
        let boxed = SalsaBox::new(&PublicKey::from(client_key), &key);
        let nonce = (&frame.payload[32..56]).into();
        let plaintext = boxed.decrypt(nonce, &frame.payload[56..]).map_err(|_| {
            io::Error::new(io::ErrorKind::PermissionDenied, "client key proof failed")
        })?;
        let info: Value = serde_json::from_slice(&plaintext)?;
        if info.get("version").and_then(Value::as_i64).unwrap_or(0) < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported DERP version",
            ));
        }
        break client_key;
    };
    verify_client(verify_url, client_key, remote)?;
    let boxed = SalsaBox::new(&PublicKey::from(client_key), &key);
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let ciphertext = boxed
        .encrypt(&nonce, json!({"version":2}).to_string().as_bytes())
        .map_err(|_| io::Error::other("encrypt server info"))?;
    let mut server_info = nonce.to_vec();
    server_info.extend(ciphertext);
    write_frame(&mut stream, 0x03, &server_info)?;
    let (sender, receiver) = mpsc::sync_channel(64);
    let sender = Arc::new(sender);
    peers.lock().unwrap().insert(client_key, sender.clone());
    let _registration = Registration {
        peers: peers.clone(),
        key: client_key,
        sender,
    };
    stream
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut last_keepalive = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        for frame in read_frames(&mut pending, MAX_PACKET + 32)? {
            handle_frame(&mut stream, &peers, client_key, frame)?;
        }
        while let Ok(packet) = receiver.try_recv() {
            write_frame(&mut stream, 0x05, &packet)?;
        }
        if last_keepalive.elapsed() >= Duration::from_secs(30) {
            write_frame(&mut stream, 0x06, &[])?;
            last_keepalive = Instant::now();
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                pending.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

struct Registration {
    peers: Peers,
    key: PeerKey,
    sender: Arc<mpsc::SyncSender<Vec<u8>>>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        let mut peers = self.peers.lock().unwrap();
        if peers
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.sender))
        {
            peers.remove(&self.key);
        }
    }
}

fn handle_frame(
    stream: &mut impl Write,
    peers: &Peers,
    source: PeerKey,
    frame: Frame,
) -> io::Result<()> {
    match frame.kind {
        0x04 if frame.payload.len() >= 32 && frame.payload.len() <= MAX_PACKET + 32 => {
            let destination: PeerKey = frame.payload[..32].try_into().unwrap();
            if let Some(sender) = peers.lock().unwrap().get(&destination).cloned() {
                let mut packet = Vec::with_capacity(frame.payload.len());
                packet.extend_from_slice(&source);
                packet.extend_from_slice(&frame.payload[32..]);
                let _ = sender.try_send(packet);
            } else {
                let mut gone = destination.to_vec();
                gone.push(1);
                write_frame(stream, 0x08, &gone)?;
            }
        }
        0x12 if frame.payload.len() == 8 => write_frame(stream, 0x13, &frame.payload)?,
        0x07 | 0x13 => (),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid DERP frame",
            ))
        }
    }
    Ok(())
}

fn verify_client(url: &str, key: PeerKey, remote: SocketAddr) -> io::Result<()> {
    let body =
        json!({"NodePublic": format!("nodekey:{}", hex(&key)), "Source": remote.ip().to_string()})
            .to_string();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .new_agent();
    let response = agent
        .post(url)
        .header("content-type", "application/json")
        .send(body.as_bytes())
        .map_err(io::Error::other)?;
    let answer: Value =
        serde_json::from_reader(response.into_body().as_reader()).map_err(io::Error::other)?;
    if answer.get("Allow").and_then(Value::as_bool) != Some(true) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "DERP admission denied",
        ));
    }
    Ok(())
}

fn read_request(stream: &mut impl Read) -> io::Result<String> {
    let mut request = Vec::new();
    while request.len() < 8192 {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        request.push(byte[0]);
        if request.ends_with(b"\r\n\r\n") {
            return String::from_utf8(request)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP request"));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "oversized HTTP request",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    fn frame(stream: &mut impl Read) -> (u8, Vec<u8>) {
        let mut header = [0u8; 5];
        stream.read_exact(&mut header).unwrap();
        let len = u32::from_be_bytes(header[1..].try_into().unwrap()) as usize;
        let mut payload = vec![0; len];
        stream.read_exact(&mut payload).unwrap();
        (header[0], payload)
    }

    fn connect(
        addr: SocketAddr,
        cert: &Certificate,
    ) -> StreamOwned<rustls::ClientConnection, TcpStream> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert.der.clone().into()).unwrap();
        let config = rustls::ClientConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let socket = TcpStream::connect(addr).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let connection =
            rustls::ClientConnection::new(Arc::new(config), "localhost".try_into().unwrap())
                .unwrap();
        let mut stream = StreamOwned::new(connection, socket);
        stream.write_all(b"GET /derp HTTP/1.1\r\nHost: localhost\r\nConnection: Upgrade\r\nUpgrade: derp\r\n\r\n").unwrap();
        let mut reader = io::BufReader::new(&mut stream);
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        stream
    }

    fn authenticate(
        stream: &mut StreamOwned<rustls::ClientConnection, TcpStream>,
        client: &SecretKey,
    ) -> PeerKey {
        let (kind, greeting) = frame(stream);
        assert_eq!(kind, 1);
        assert_eq!(&greeting[..8], b"DERP\xf0\x9f\x94\x91");
        let server_key: PeerKey = greeting[8..40].try_into().unwrap();
        let boxed = SalsaBox::new(&PublicKey::from(server_key), client);
        let nonce = SalsaBox::generate_nonce(&mut OsRng);
        let encrypted = boxed
            .encrypt(&nonce, b"{\"version\":2}".as_slice())
            .unwrap();
        let mut payload = client.public_key().as_bytes().to_vec();
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&encrypted);
        write_frame(stream, 2, &payload).unwrap();
        let (kind, server_info) = frame(stream);
        assert_eq!(kind, 3);
        let decrypted = boxed
            .decrypt((&server_info[..24]).into(), &server_info[24..])
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&decrypted).unwrap()["version"],
            2
        );
        server_key
    }

    #[test]
    fn authenticated_peers_relay_and_admission_is_required() {
        let dir = tempfile::tempdir().unwrap();
        let verifier = TcpListener::bind("127.0.0.1:0").unwrap();
        let verify_addr = verifier.local_addr().unwrap();
        let verify = thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = verifier.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reader = io::BufReader::new(&mut socket);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert!(first.contains("POST /verify"));
                let mut content_len = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length: ")
                    {
                        content_len = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; content_len];
                reader.read_exact(&mut body).unwrap();
                assert!(String::from_utf8_lossy(&body).contains("nodekey:"));
                drop(reader);
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\n{\"Allow\":true}")
                    .unwrap();
            }
        });
        let service = Service::start(Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            stun_listen: "127.0.0.1:0".parse().unwrap(),
            hostname: "localhost".into(),
            verify_url: format!("http://{verify_addr}/verify"),
            state_dir: dir.path().to_owned(),
        })
        .unwrap();
        let cert = Certificate::load_or_create(dir.path(), "localhost").unwrap();
        assert_eq!(service.certificate_pin, cert.pin());
        let key_a = SecretKey::generate(&mut OsRng);
        let key_b = SecretKey::generate(&mut OsRng);
        let mut a = connect(service.derp_addr, &cert);
        authenticate(&mut a, &key_a);
        let mut b = connect(service.derp_addr, &cert);
        authenticate(&mut b, &key_b);
        let mut sent = key_b.public_key().as_bytes().to_vec();
        sent.extend_from_slice(b"wireguard");
        write_frame(&mut a, 4, &sent).unwrap();
        let (kind, received) = frame(&mut b);
        assert_eq!(kind, 5);
        assert_eq!(&received[..32], key_a.public_key().as_bytes());
        assert_eq!(&received[32..], b"wireguard");
        verify.join().unwrap();
        let start = Instant::now();
        drop(service);
        assert!(start.elapsed() < Duration::from_secs(2));
        let mut one = [0];
        assert!(matches!(a.read(&mut one), Ok(0) | Err(_)));
    }

    #[test]
    fn verify_endpoint_must_be_exact_loopback_url() {
        assert!(local_verify_url("http://127.0.0.1:7480/verify"));
        assert!(local_verify_url("http://[::1]:7480/verify"));
        assert!(local_verify_url("http://localhost:7480/verify"));
        assert!(!local_verify_url("http://localhost:7480.evil/verify"));
        assert!(!local_verify_url("http://192.0.2.1:7480/verify"));
        assert!(!local_verify_url("http://127.0.0.1:7480/anything"));
    }
}
