//! Native projection of the shared public-code QR modules.

/// Row-major dark modules; platform rendering adds a four-module quiet zone.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MobileQrMatrix {
    /// Positive square side in modules.
    pub size: i32,
    /// Exactly size squared modules, with true for a dark module.
    pub modules: Vec<bool>,
}

impl From<dyapp_client::qr::QrMatrix> for MobileQrMatrix {
    fn from(matrix: dyapp_client::qr::QrMatrix) -> Self {
        Self {
            size: matrix.size,
            modules: matrix.modules,
        }
    }
}
