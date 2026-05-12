use std::io::Read;

use crate::LocalChecksumResult;

pub fn calculate_local_checksum(path: &str) -> Result<LocalChecksumResult, String> {
    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open file for checksum: {error}"))?;
    let mut md5_context = md5::Context::new();
    let mut sha256 = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read file for checksum: {error}"))?;
        if read == 0 {
            break;
        }
        md5_context.consume(&buffer[..read]);
        sha256.update(&buffer[..read]);
    }
    Ok(LocalChecksumResult {
        md5: format!("{:x}", md5_context.compute()),
        sha256: format!("{:x}", sha256.finalize()),
    })
}
