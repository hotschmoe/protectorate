use protectorate_tailnet_wg::{Identity, Initiator, Responder};
use std::io::Write;
use std::process::{Command, Stdio};

fn go(mode: &str, input: Option<&[u8]>) -> Vec<u8> {
    let helper = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/interop.go");
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../..");
    let scratch = std::env::temp_dir().join(format!(
        "protectorate-wg-go-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&scratch).unwrap();
    let modfile = scratch.join("go.mod");
    std::fs::copy(format!("{root}/go.mod"), &modfile).unwrap();
    std::fs::copy(format!("{root}/go.sum"), scratch.join("go.sum")).unwrap();
    let mut child = Command::new("go")
        .args([
            "run",
            "-mod=mod",
            &format!("-modfile={}", modfile.display()),
            helper,
            mode,
        ])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("go run available");
    if let Some(packet) = input {
        let mut stdin = child.stdin.take().unwrap();
        for byte in packet {
            write!(stdin, "{byte:02x}").unwrap();
        }
        writeln!(stdin).unwrap();
    } else {
        drop(child.stdin.take());
    }
    let output = child.wait_with_output().unwrap();
    std::fs::remove_dir_all(&scratch).unwrap();
    assert!(
        output.status.success(),
        "wireguard-go exited with {}",
        output.status
    );
    decode_hex(std::str::from_utf8(&output.stdout).unwrap().trim())
}

fn decode_hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
#[ignore = "requires cached wireguard-go module and Go toolchain"]
fn wireguard_go_accepts_rust_initiation_and_rust_accepts_response() {
    let alice = Identity::from_private([1; 32]).unwrap();
    let bob = Identity::from_private([2; 32]).unwrap();
    let (initiator, packet) =
        Initiator::with_index(&alice, bob.public_key(), [0; 32], [1; 12], 17).unwrap();
    let response = go("respond", Some(&packet));
    let session = initiator.consume_response(&response).unwrap();
    assert_eq!(session.local_index(), 17);
}

#[test]
#[ignore = "requires cached wireguard-go module and Go toolchain"]
fn rust_accepts_wireguard_go_initiation() {
    let alice = Identity::from_private([1; 32]).unwrap();
    let bob = Identity::from_private([2; 32]).unwrap();
    let mut responder = Responder::new(bob, alice.public_key(), [0; 32]).unwrap();
    let initiation = go("initiate", None);
    let (_response, session) = responder.accept(&initiation).unwrap().respond().unwrap();
    assert_ne!(session.local_index(), session.remote_index());
}
