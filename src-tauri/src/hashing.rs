use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// BLAKE3 hash of a file's contents, streamed in chunks so multi-gigabyte
/// backup files don't get loaded into memory at once.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
