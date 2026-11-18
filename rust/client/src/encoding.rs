//! Hex decoding for bounded protocol identifiers and addresses.

use crate::Error;

pub(crate) fn unhex(text: &str) -> Result<Vec<u8>, Error> {
    if !text.len().is_multiple_of(2) {
        return Err(Error::Reply("odd-length hex"));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i.saturating_add(2))
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or(Error::Reply("bad hex"))
        })
        .collect()
}
