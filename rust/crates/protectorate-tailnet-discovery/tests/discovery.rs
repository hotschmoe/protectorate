use protectorate_tailnet_discovery::{DiscoIdentity, DiscoPublic};
use std::{fs, sync::Arc, thread};

#[test]
fn persists_public_key_with_tailscale_wire_format() {
    let temp = tempfile::tempdir().unwrap();
    let first = DiscoIdentity::open(temp.path()).unwrap();
    let public = first.public_key().to_string();
    assert!(public.starts_with("discokey:"));
    assert_eq!(public.len(), "discokey:".len() + 64);
    assert_eq!(
        DiscoPublic::parse(&public).unwrap().bytes(),
        first.public_key().bytes()
    );
    assert!(!format!("{first:?}").contains("privkey:"));
    let private = first.private_key_bytes();
    assert_eq!(private[0] & 7, 0);
    assert_eq!(private[31] & 0xc0, 0x40);
    drop(first);
    assert_eq!(
        DiscoIdentity::open(temp.path())
            .unwrap()
            .public_key()
            .to_string(),
        public
    );
    let file = fs::read_to_string(temp.path().join("disco.key")).unwrap();
    assert!(file.starts_with("privkey:"));
    assert!(file.ends_with('\n'));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(temp.path().join("disco.key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[test]
fn concurrent_creators_share_one_identity() {
    let temp = tempfile::tempdir().unwrap();
    let path = Arc::new(temp.path().to_path_buf());
    let threads: Vec<_> = (0..12)
        .map(|_| {
            let path = Arc::clone(&path);
            thread::spawn(move || {
                DiscoIdentity::open(&*path)
                    .unwrap()
                    .public_key()
                    .to_string()
            })
        })
        .collect();
    let mut threads = threads.into_iter();
    let public = threads.next().unwrap().join().unwrap();
    for handle in threads {
        assert_eq!(handle.join().unwrap(), public);
    }
}

#[test]
fn rejects_corrupt_or_exposed_state_without_replacing_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("disco.key");
    fs::write(&path, b"privkey:bad\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(DiscoIdentity::open(temp.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"privkey:bad\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(DiscoIdentity::open(temp.path()).is_err());
    }
}

#[cfg(unix)]
#[test]
fn rejects_symlink_key() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    symlink("missing", temp.path().join("disco.key")).unwrap();
    assert!(DiscoIdentity::open(temp.path()).is_err());
}
