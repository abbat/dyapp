/// Architecture linting tool for P2P correctness
/// Validates that code adheres to P2P design principles

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

mod rules;
mod violations;

use rules::{check_no_centralized_api, check_encrypted_data};
use violations::Violation;

/// Lint configuration
struct LintConfig {
    /// Root directory to scan
    root_dir: PathBuf,
    /// Fail on any violation
    strict: bool,
    /// Output format
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Text,
    Json,
}

impl Default for LintConfig {
    fn default() -> Self {
        LintConfig {
            root_dir: PathBuf::from("."),
            strict: true,
            format: OutputFormat::Text,
        }
    }
}

/// Run all linting rules
fn run_linting(config: &LintConfig) -> Vec<Violation> {
    let mut violations = Vec::new();

    println!("🔍 Scanning for P2P architecture violations...\n");

    // Scan source files
    if let Ok(entries) = fs::read_dir(&config.root_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "rs") {
                if let Ok(content) = fs::read_to_string(&path) {
                    // Run all checks
                    violations.extend(check_no_centralized_api(&path, &content));
                    violations.extend(check_encrypted_data(&path, &content));
                }
            }
        }
    }

    violations
}

/// Print violations based on format
fn print_violations(violations: &[Violation], format: OutputFormat) {
    match format {
        OutputFormat::Text => {
            for (i, violation) in violations.iter().enumerate() {
                println!(
                    "{}. {} ({}:{})",
                    i + 1,
                    violation.message,
                    violation.file.display(),
                    violation.line
                );
                println!("   Rule: {}", violation.rule);
                println!("   Severity: {}", violation.severity);
                println!();
            }
        }
        OutputFormat::Json => {
            // JSON output for CI integration
            println!("[");
            for (i, violation) in violations.iter().enumerate() {
                print!(
                    r#"  {{
    "rule": "{}",
    "severity": "{}",
    "file": "{}",
    "line": {},
    "message": "{}"
  }}"#,
                    violation.rule,
                    violation.severity,
                    violation.file.display(),
                    violation.line,
                    violation.message
                );
                if i < violations.len() - 1 {
                    println!(",");
                } else {
                    println!();
                }
            }
            println!("]");
        }
    }
}

/// Main entry point
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config = parse_args(&args);

    println!("📦 DYApp - Architecture Linting Tool\n");
    println!("Scanning: {}\n", config.root_dir.display());

    let violations = run_linting(&config);

    println!("\n📊 Results:\n");
    println!("Total violations: {}\n", violations.len());

    if !violations.is_empty() {
        print_violations(&violations, config.format);

        if config.strict {
            eprintln!("\n❌ Architecture linting failed!");
            process::exit(1);
        } else {
            println!("\n⚠️  Architecture linting warnings found");
            process::exit(0);
        }
    } else {
        println!("✅ No architecture violations found!");
        process::exit(0);
    }
}

/// Parse command line arguments
fn parse_args(args: &[String]) -> LintConfig {
    let mut config = LintConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--root" | "-r" => {
                if i + 1 < args.len() {
                    config.root_dir = PathBuf::from(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("Error: --root requires a path");
                    process::exit(1);
                }
            }
            "--strict" | "-s" => {
                config.strict = true;
                i += 1;
            }
            "--json" => {
                config.format = OutputFormat::Json;
                i += 1;
            }
            "--help" | "-h" => {
                print_help();
                process::exit(0);
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                print_help();
                process::exit(1);
            }
        }
    }

    config
}

/// Print help message
fn print_help() {
    println!(
        r#"
Architecture Linting Tool - P2P Correctness Validation

Usage: arch-lint [OPTIONS]

Options:
  -r, --root <PATH>    Root directory to scan (default: current dir)
  -s, --strict         Fail on any violation (default: true)
  --json               Output violations as JSON
  -h, --help           Print this help message

Examples:
  # Scan current directory
  cargo run --bin arch-lint

  # Scan specific directory
  cargo run --bin arch-lint -- --root src/

  # JSON output for CI
  cargo run --bin arch-lint -- --json

Rules:
  1. No centralized API calls in P2P modules
     - Blocks std::http, reqwest, etc.
     - Allows only p2p-net functions

  2. Encrypted data enforcement
     - All sensitive data must be marked [encrypted]
     - Prevents plaintext transmission

"#
    );
}
