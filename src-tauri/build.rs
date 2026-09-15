use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();
    write_signing_secret();
}

/// Bakes the license-verification HMAC secret into the binary at build
/// time instead of hardcoding it in tracked source (`src/license.rs` used
/// to do that — anyone with repo access could read it straight off GitHub
/// and mint their own valid license keys, no reverse-engineering needed).
/// CI supplies it via the `BVPR_SIGNING_SECRET` env var (a repo secret,
/// same pattern as `TAURI_SIGNING_PRIVATE_KEY`); a local dev build falls
/// back to the gitignored `keygen/signing-key.secret` file so `cargo build`
/// keeps working without extra setup.
fn write_signing_secret() {
    println!("cargo:rerun-if-env-changed=BVPR_SIGNING_SECRET");

    let secret: [u8; 32] = match env::var("BVPR_SIGNING_SECRET") {
        Ok(hex) => decode_hex(&hex).unwrap_or_else(|e| {
            panic!("BVPR_SIGNING_SECRET is set but invalid: {e}")
        }),
        Err(_) => {
            let path = local_secret_path();
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = fs::read(&path).unwrap_or_else(|_| {
                panic!(
                    "No BVPR_SIGNING_SECRET env var and no secret file at {} — \
                     run `cargo run -- genkey` in keygen/ first for a local build, \
                     or set BVPR_SIGNING_SECRET for CI.",
                    path.display()
                )
            });
            bytes.try_into().unwrap_or_else(|v: Vec<u8>| {
                panic!("{} must be exactly 32 bytes, got {}", path.display(), v.len())
            })
        }
    };

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let dest = Path::new(&out_dir).join("signing_secret.rs");
    fs::write(&dest, format!("pub const SHARED_SECRET: [u8; 32] = {secret:?};\n"))
        .expect("failed to write generated signing_secret.rs");
}

fn local_secret_path() -> PathBuf {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
    Path::new(&manifest_dir).join("../keygen/signing-key.secret")
}

fn decode_hex(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("expected 64 hex characters (32 bytes), got {}", s.len()));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in out.iter_mut().enumerate() {
        *chunk = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("invalid hex at byte {i}"))?;
    }
    Ok(out)
}
