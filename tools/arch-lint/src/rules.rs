/// Linting rules for P2P architecture validation
use std::path::{Path, PathBuf};
use crate::violations::Violation;

/// Check for centralized API calls in P2P modules
pub fn check_no_centralized_api(path: &Path, content: &str) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Only check P2P modules
    if !path.to_string_lossy().contains("p2p_net") && !path.to_string_lossy().contains("bootstrap") {
        return violations;
    }

    let forbidden_patterns = vec![
        ("std::http", "Centralized HTTP API"),
        ("reqwest", "Centralized HTTP client"),
        ("hyper::", "Centralized HTTP framework"),
        ("axum::", "Centralized web framework"),
        ("tokio::net::TcpListener", "Centralized TCP listener (use libp2p instead)"),
    ];

    for (pattern, reason) in forbidden_patterns {
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(pattern) && !line.trim_start().starts_with("//") {
                violations.push(Violation {
                    rule: "NO_CENTRALIZED_API".to_string(),
                    severity: "error".to_string(),
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    message: format!(
                        "Found centralized API usage: {} - {}. Use P2P alternatives only.",
                        pattern, reason
                    ),
                });
            }
        }
    }

    violations
}

/// Check that sensitive data is marked as encrypted
pub fn check_encrypted_data(path: &Path, content: &str) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Check for unencrypted sensitive data
    let sensitive_patterns = vec![
        ("password", "Password data"),
        ("private_key", "Private cryptographic key"),
        ("secret", "Secret value"),
        ("token", "Authentication token"),
    ];

    for (pattern, data_type) in sensitive_patterns {
        for (line_num, line) in content.lines().enumerate() {
            if line.contains(pattern) && !line.trim_start().starts_with("//") {
                // Check if marked as encrypted
                if !line.contains("[encrypted]") && !line.contains("crypto::")
                    && !line.contains("encrypt")
                {
                    // Skip if already in encrypted context
                    let context = &content.lines().nth(line_num.saturating_sub(2)).unwrap_or("");
                    if !context.contains("encrypted") {
                        violations.push(Violation {
                            rule: "UNENCRYPTED_DATA".to_string(),
                            severity: "error".to_string(),
                            file: path.to_path_buf(),
                            line: line_num + 1,
                            message: format!(
                                "Potentially unencrypted {}: '{}'. Mark sensitive data with [encrypted] or use crypto:: module",
                                data_type, pattern
                            ),
                        });
                    }
                }
            }
        }
    }

    violations
}
