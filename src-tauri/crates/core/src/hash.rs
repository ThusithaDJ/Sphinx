use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::Result;

/// Content-hash a file with BLAKE3 (fast, streaming, no crypto overkill needed
/// here since this is for dedupe, not security). Reads in fixed-size chunks so
/// memory use stays flat regardless of file size (important for large video
/// assets).
pub fn hash_file(path: impl AsRef<Path>) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 1 << 16]; // 64 KiB chunks
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn same_content_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.bin");
        let p2 = dir.path().join("b.bin");
        std::fs::File::create(&p1)
            .unwrap()
            .write_all(b"hello sphinx")
            .unwrap();
        std::fs::File::create(&p2)
            .unwrap()
            .write_all(b"hello sphinx")
            .unwrap();

        assert_eq!(hash_file(&p1).unwrap(), hash_file(&p2).unwrap());
    }

    #[test]
    fn different_content_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.bin");
        let p2 = dir.path().join("b.bin");
        std::fs::File::create(&p1).unwrap().write_all(b"a").unwrap();
        std::fs::File::create(&p2).unwrap().write_all(b"b").unwrap();

        assert_ne!(hash_file(&p1).unwrap(), hash_file(&p2).unwrap());
    }
}
