use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Video codec identifiers used by the preference helper.
pub enum CodecFormat {
    /// VP8 video.
    VP8,
    /// VP9 video.
    VP9,
    /// H.264 video.
    H264,
    /// AV1 video.
    AV1,
}

impl CodecFormat {
    /// Returns the codec's video MIME type.
    pub fn mime_type(&self) -> &str {
        match self {
            Self::VP8 => "video/VP8",
            Self::VP9 => "video/VP9",
            Self::H264 => "video/H264",
            Self::AV1 => "video/AV1",
        }
    }

    /// Returns the static H.264 classification; does not probe hardware.
    pub fn is_hardware_accelerated(&self) -> bool {
        matches!(self, Self::H264)
    }

    /// Returns the static non-H.264 classification; does not assess licensing.
    pub fn is_open_source(&self) -> bool {
        !matches!(self, Self::H264)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Local codec description; fields are not validated.
pub struct CodecInfo {
    /// Codec identifier.
    pub format: CodecFormat,
    /// RTP clock rate in hertz.
    pub clock_rate: u32,
    /// RTP payload type.
    pub payload_type: u8,
    /// Optional codec parameter string.
    pub parameters: Option<String>,
}

impl CodecInfo {
    /// Returns VP8 at 90 kHz with payload type 96.
    pub fn vp8() -> Self {
        Self {
            format: CodecFormat::VP8,
            clock_rate: 90000,
            payload_type: 96,
            parameters: None,
        }
    }

    /// Returns VP9 at 90 kHz with payload type 98.
    pub fn vp9() -> Self {
        Self {
            format: CodecFormat::VP9,
            clock_rate: 90000,
            payload_type: 98,
            parameters: None,
        }
    }

    /// Returns H.264 at 90 kHz, payload 97, profile-level-id 42e01e.
    pub fn h264() -> Self {
        Self {
            format: CodecFormat::H264,
            clock_rate: 90000,
            payload_type: 97,
            parameters: Some("profile-level-id=42e01e".to_string()),
        }
    }

    /// Returns AV1 at 90 kHz with payload type 99.
    pub fn av1() -> Self {
        Self {
            format: CodecFormat::AV1,
            clock_rate: 90000,
            payload_type: 99,
            parameters: None,
        }
    }
}

#[derive(Debug)]
/// Matches formats in local preference order, ignoring other codec fields.
/// This helper does not configure encoders or negotiate SDP.
pub struct CodecNegotiation {
    local_codecs: Vec<CodecInfo>,
    remote_codecs: Vec<CodecInfo>,
    agreed_codec: Option<CodecInfo>,
}

impl CodecNegotiation {
    /// Starts with local preferences and no remote codecs or agreement.
    pub fn new(local_codecs: Vec<CodecInfo>) -> Self {
        Self {
            local_codecs,
            remote_codecs: Vec::new(),
            agreed_codec: None,
        }
    }

    /// Returns VP8, H.264 and VP9 in preference order; excludes AV1.
    pub fn default_codecs() -> Vec<CodecInfo> {
        vec![CodecInfo::vp8(), CodecInfo::h264(), CodecInfo::vp9()]
    }

    /// Starts with the default local preferences.
    pub fn with_defaults() -> Self {
        Self::new(Self::default_codecs())
    }

    /// Replaces remote codecs and selects the first matching local format.
    pub fn set_remote_codecs(&mut self, remote_codecs: Vec<CodecInfo>) {
        self.remote_codecs = remote_codecs;
        self.negotiate();
    }

    fn negotiate(&mut self) {
        self.agreed_codec = self
            .local_codecs
            .iter()
            .find(|local| {
                self.remote_codecs
                    .iter()
                    .any(|remote| remote.format == local.format)
            })
            .cloned();
    }

    /// Returns the selected local description, if formats overlap.
    pub fn agreed_codec(&self) -> Option<&CodecInfo> {
        self.agreed_codec.as_ref()
    }

    /// Borrows local codec preferences.
    pub fn local_codecs(&self) -> &[CodecInfo] {
        &self.local_codecs
    }

    /// Borrows the supplied remote descriptions.
    pub fn remote_codecs(&self) -> &[CodecInfo] {
        &self.remote_codecs
    }

    /// Reports whether a format match exists.
    pub fn has_agreed(&self) -> bool {
        self.agreed_codec.is_some()
    }
}

impl Default for CodecNegotiation {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_format_mime() {
        assert_eq!(CodecFormat::VP8.mime_type(), "video/VP8");
        assert_eq!(CodecFormat::H264.mime_type(), "video/H264");
    }

    #[test]
    fn test_codec_info_creation() {
        let vp8 = CodecInfo::vp8();
        assert_eq!(vp8.format, CodecFormat::VP8);
        assert_eq!(vp8.clock_rate, 90000);
    }

    #[test]
    fn test_codec_negotiation() {
        let mut negotiation = CodecNegotiation::with_defaults();
        assert_eq!(negotiation.local_codecs().len(), 3);
        assert!(!negotiation.has_agreed());

        let remote = vec![CodecInfo::h264()];
        negotiation.set_remote_codecs(remote);

        assert!(negotiation.has_agreed());
        assert_eq!(
            negotiation.agreed_codec().unwrap().format,
            CodecFormat::H264
        );
    }

    #[test]
    fn test_codec_negotiation_without_common_codec_fails() {
        let local = vec![CodecInfo::vp8()];
        let mut negotiation = CodecNegotiation::new(local);

        let remote = vec![CodecInfo::h264(), CodecInfo::av1()];
        negotiation.set_remote_codecs(remote);

        assert!(!negotiation.has_agreed());
        assert!(negotiation.agreed_codec().is_none());
    }

    #[test]
    fn test_codec_hardware_acceleration() {
        assert!(CodecFormat::H264.is_hardware_accelerated());
        assert!(!CodecFormat::VP8.is_hardware_accelerated());
    }
}
