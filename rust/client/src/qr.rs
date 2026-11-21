//! Public-code QR modules shared by native and webview presentation adapters.

use qrcodegen::{QrCode, QrCodeEcc};

/// Row-major QR modules without a quiet zone; presentation adds the standard outer margin.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QrMatrix {
    /// Positive square side in modules, as supplied by the encoder.
    pub size: i32,
    /// Dark modules are true; the length is the square of the side.
    pub modules: Vec<bool>,
}

/// Encode exact public bytes with the same medium correction level on every client.
///
/// Returns `None` when the payload exceeds QR capacity. It does not parse or authorize a code.
#[must_use]
pub fn matrix(payload: &[u8]) -> Option<QrMatrix> {
    let qr = QrCode::encode_binary(payload, QrCodeEcc::Medium).ok()?;
    let size = qr.size();
    let qr = &qr;
    let modules = (0..size)
        .flat_map(|y| (0..size).map(move |x| qr.get_module(x, y)))
        .collect();
    Some(QrMatrix { size, modules })
}

#[cfg(test)]
#[path = "qr/tests.rs"]
mod tests;
