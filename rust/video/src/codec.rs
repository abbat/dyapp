use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodecFormat {
    VP8,
    VP9,
    H264,
    AV1,
}

impl CodecFormat {
    pub fn mime_type(&self) -> &str {
        match self {
            Self::VP8 => "video/VP8",
            Self::VP9 => "video/VP9",
            Self::H264 => "video/H264",
            Self::AV1 => "video/AV1",
        }
    }

    pub fn is_hardware_accelerated(&self) -> bool {
        matches!(self, Self::H264)
    }

    pub fn is_open_source(&self) -> bool {
        !matches!(self, Self::H264)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecInfo {
    pub format: CodecFormat,
    pub clock_rate: u32,
    pub payload_type: u8,
    pub parameters: Option<String>,
}

impl CodecInfo {
    pub fn vp8() -> Self {
        Self {
            format: CodecFormat::VP8,
            clock_rate: 90000,
            payload_type: 96,
            parameters: None,
        }
    }

    pub fn vp9() -> Self {
        Self {
            format: CodecFormat::VP9,
            clock_rate: 90000,
            payload_type: 98,
            parameters: None,
        }
    }

    pub fn h264() -> Self {
        Self {
            format: CodecFormat::H264,
            clock_rate: 90000,
            payload_type: 97,
            parameters: Some("profile-level-id=42e01e".to_string()),
        }
    }

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
pub struct CodecNegotiation {
    local_codecs: Vec<CodecInfo>,
    remote_codecs: Vec<CodecInfo>,
    agreed_codec: Option<CodecInfo>,
}

impl CodecNegotiation {
    pub fn new(local_codecs: Vec<CodecInfo>) -> Self {
        Self {
            local_codecs,
            remote_codecs: Vec::new(),
            agreed_codec: None,
        }
    }

    pub fn default_codecs() -> Vec<CodecInfo> {
        vec![CodecInfo::vp8(), CodecInfo::h264(), CodecInfo::vp9()]
    }

    pub fn with_defaults() -> Self {
        Self::new(Self::default_codecs())
    }

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

        if self.agreed_codec.is_none() && !self.local_codecs.is_empty() {
            self.agreed_codec = Some(self.local_codecs[0].clone());
        }
    }

    pub fn agreed_codec(&self) -> Option<&CodecInfo> {
        self.agreed_codec.as_ref()
    }

    pub fn local_codecs(&self) -> &[CodecInfo] {
        &self.local_codecs
    }

    pub fn remote_codecs(&self) -> &[CodecInfo] {
        &self.remote_codecs
    }

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
    fn test_codec_negotiation_fallback() {
        let local = vec![CodecInfo::vp8()];
        let mut negotiation = CodecNegotiation::new(local);

        let remote = vec![CodecInfo::h264(), CodecInfo::av1()];
        negotiation.set_remote_codecs(remote);

        assert!(negotiation.has_agreed());
        assert_eq!(negotiation.agreed_codec().unwrap().format, CodecFormat::VP8);
    }

    #[test]
    fn test_codec_hardware_acceleration() {
        assert!(CodecFormat::H264.is_hardware_accelerated());
        assert!(!CodecFormat::VP8.is_hardware_accelerated());
    }
}
