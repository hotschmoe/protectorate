//! Embeds member for the target (`$OUT_DIR/member.rs`, through `pt_embed::build`): from
//! `PROTECTORATE_NET_DIR`, else from `go/bin`, refreshed first by `scripts/net-build.sh` when
//! Go is on PATH and the Go sources are newer. Without it the node needs `--member PATH`.

use std::path::{Path, PathBuf};

fn main() {
    use pt_embed::build::{Target, blob_dir, literal};
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let t = Target::current();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = blob_dir(&root, &["member"], &t);
    let src = match literal("pt_embed::Embedded", "member", &dir, &t, &out) {
        Ok(l) => format!(
            "pub static MEMBER: Option<pt_embed::Embedded> = {l};\npub static MEMBER_MISSING: &str = \"\";\n"
        ),
        Err(e) => {
            println!("cargo:warning=member is not embedded ({e}); the node needs --member PATH");
            format!(
                "pub static MEMBER: Option<pt_embed::Embedded> = None;\npub static MEMBER_MISSING: &str = {e:?};\n"
            )
        }
    };
    std::fs::write(out.join("member.rs"), src).expect("write member.rs");
}
