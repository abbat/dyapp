//! Compiles `proto/profile.proto` into the prost types of this crate.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto");
    let files = protox::compile(["profile.proto"], ["../../proto"])?;
    let mut config = prost_build::Config::new();
    // Practice B3: round-trip properties generate every message.
    if std::env::var_os("CARGO_FEATURE_PROPTEST").is_some() {
        config.type_attribute(".", "#[derive(::proptest_derive::Arbitrary)]");
    }
    config.compile_fds(files)?;
    Ok(())
}
