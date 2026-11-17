//! Canonical lowercase hexadecimal for identity and blob identifiers.

/// Encodes every byte as two lowercase hexadecimal digits, retaining leading zeros.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|&byte| {
            [
                char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'),
                char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn every_byte_and_leading_zero_have_the_canonical_representation() {
        assert_eq!(encode(&[]), "");
        assert_eq!(encode(&[0, 0x0f, 0x10, 0xab, 0xff]), "000f10abff");
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(encode(&[byte]), format!("{byte:02x}"));
        }
    }
}
