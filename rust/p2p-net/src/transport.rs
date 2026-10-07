use std::net::SocketAddr;

/// Quinn/QUIC transport configuration
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// Local bind address (0.0.0.0:0 for auto-assign)
    pub local_addr: SocketAddr,

    /// Connection timeout (milliseconds)
    pub connection_timeout_ms: u64,

    /// Idle timeout (milliseconds)
    pub idle_timeout_ms: u64,

    /// Max concurrent streams
    pub max_concurrent_streams: u64,

    /// Max datagram size (bytes)
    pub max_datagram_size: u16,

    /// Enable keep-alive
    pub keep_alive_enabled: bool,

    /// Keep-alive interval (seconds)
    pub keep_alive_interval_secs: u32,
}

impl TransportConfig {
    pub fn default_for_mobile() -> Self {
        Self {
            local_addr: "0.0.0.0:0".parse().expect("Invalid default address"),
            connection_timeout_ms: 30000, // 30 seconds
            idle_timeout_ms: 60000,       // 1 minute
            max_concurrent_streams: 16,
            max_datagram_size: 1200,
            keep_alive_enabled: true,
            keep_alive_interval_secs: 30,
        }
    }

    pub fn default_for_desktop() -> Self {
        Self {
            local_addr: "0.0.0.0:0".parse().expect("Invalid default address"),
            connection_timeout_ms: 10000, // 10 seconds (more resources)
            idle_timeout_ms: 120000,      // 2 minutes
            max_concurrent_streams: 64,
            max_datagram_size: 1500,
            keep_alive_enabled: false,
            keep_alive_interval_secs: 0,
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::default_for_mobile()
    }
}

/// QuicTransport wraps quinn::Endpoint
pub struct QuicTransport {
    config: TransportConfig,
}

impl QuicTransport {
    pub fn new(config: TransportConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &TransportConfig {
        &self.config
    }

    pub async fn bind(&self) -> Result<(), String> {
        // TODO: Implement quinn endpoint binding
        Ok(())
    }

    pub async fn connect(&self, _remote_addr: SocketAddr) -> Result<(), String> {
        // TODO: Implement QUIC connection
        Ok(())
    }

    pub async fn listen(&self) -> Result<(), String> {
        // TODO: Implement QUIC listener
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mobile_config() {
        let config = TransportConfig::default_for_mobile();
        assert_eq!(config.connection_timeout_ms, 30000);
        assert!(config.keep_alive_enabled);
    }

    #[test]
    fn test_desktop_config() {
        let config = TransportConfig::default_for_desktop();
        assert_eq!(config.connection_timeout_ms, 10000);
        assert!(!config.keep_alive_enabled);
        assert!(config.max_concurrent_streams > 16);
    }

    #[test]
    fn test_default_config() {
        let config = TransportConfig::default();
        assert!(config.keep_alive_enabled); // Should default to mobile
    }

    #[test]
    fn test_quic_transport_creation() {
        let config = TransportConfig::default_for_mobile();
        let transport = QuicTransport::new(config.clone());
        assert_eq!(transport.config().connection_timeout_ms, 30000);
    }

    #[test]
    fn test_quic_transport_config() {
        let config = TransportConfig::default_for_desktop();
        let transport = QuicTransport::new(config);
        let cfg = transport.config();
        assert_eq!(cfg.max_concurrent_streams, 64);
    }

    #[tokio::test]
    async fn test_quic_transport_bind() {
        let config = TransportConfig::default();
        let transport = QuicTransport::new(config);
        let result = transport.bind().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_quic_transport_connect() {
        let config = TransportConfig::default();
        let transport = QuicTransport::new(config);
        let addr = "127.0.0.1:5000".parse().unwrap();
        let result = transport.connect(addr).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_quic_transport_listen() {
        let config = TransportConfig::default();
        let transport = QuicTransport::new(config);
        let result = transport.listen().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_mobile_vs_desktop_config() {
        let mobile = TransportConfig::default_for_mobile();
        let desktop = TransportConfig::default_for_desktop();

        assert!(mobile.keep_alive_enabled);
        assert!(!desktop.keep_alive_enabled);
        assert!(desktop.connection_timeout_ms < mobile.connection_timeout_ms);
    }

    #[test]
    fn test_transport_config_addr_parsing() {
        let config = TransportConfig::default();
        assert_eq!(config.local_addr.ip().to_string(), "0.0.0.0");
    }
}
