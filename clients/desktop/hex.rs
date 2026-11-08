//! Lowercase hexadecimal for native identifiers and fixture hashes.
#[must_use]
pub(crate) fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|&byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn lowercase_hex_preserves_every_byte_and_leading_zero() {
        assert_eq!(encode(&[]), "");
        assert_eq!(encode(&[0, 0x0f, 0x10, 0xab, 0xff]), "000f10abff");
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(encode(&[byte]), format!("{byte:02x}"));
        }
    }
}
