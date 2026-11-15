//! Bounded PNG header admission; compressed pixels remain the platform decoder's responsibility.

use super::LookupError;

pub(super) fn dimensions(bytes: &[u8]) -> Result<(u32, u32), LookupError> {
    let header: &[u8; 33] = bytes
        .get(..33)
        .and_then(|header| header.try_into().ok())
        .ok_or(LookupError::Corrupted)?;
    if &header[..16] != b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"
        || !bytes.ends_with(b"\0\0\0\0IEND\xae\x42\x60\x82")
        || crc32(&header[12..29]).to_be_bytes() != header[29..33]
        || header[26] != 0
        || header[27] != 0
        || header[28] > 1
        || !matches!(
            (header[25], header[24]),
            (0, 1 | 2 | 4 | 8 | 16) | (2 | 4 | 6, 8 | 16) | (3, 1 | 2 | 4 | 8)
        )
    {
        return Err(LookupError::Corrupted);
    }
    Ok((
        u32::from_be_bytes([header[16], header[17], header[18], header[19]]),
        u32::from_be_bytes([header[20], header[21], header[22], header[23]]),
    ))
}

// IEEE CRC-32 over the fixed 17-byte IHDR type/data, never over unbounded input.
pub(super) fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &byte| {
        (0..8).fold(crc ^ u32::from(byte), |crc, _| {
            (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg())
        })
    })
}
