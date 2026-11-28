use sha2::{Digest, Sha256};

use crate::LocalChecksumResult;

pub struct FileChecksums {
    md5: md5::Context,
    sha256: Sha256,
}

impl Default for FileChecksums {
    fn default() -> Self {
        Self {
            md5: md5::Context::new(),
            sha256: Sha256::new(),
        }
    }
}

impl FileChecksums {
    pub fn update(&mut self, bytes: &[u8]) {
        self.md5.consume(bytes);
        self.sha256.update(bytes);
    }

    pub fn finish(self) -> LocalChecksumResult {
        LocalChecksumResult {
            md5: format!("{:x}", self.md5.compute()),
            sha256: format!("{:x}", self.sha256.finalize()),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn calculate_local_checksum(path: &str) -> Result<LocalChecksumResult, String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open file for checksum: {error}"))?;
    let mut checksums = FileChecksums::default();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read file for checksum: {error}"))?;
        if read == 0 {
            break;
        }
        checksums.update(&buffer[..read]);
    }
    Ok(checksums.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_checksums_include_every_chunk_in_order() {
        let mut sums = FileChecksums::default();
        sums.update(b"a");
        sums.update(b"bc");
        let result = sums.finish();
        assert_eq!(result.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            result.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
