fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto");
    let files = protox::compile(["identity.proto"], ["../../proto"])?;
    prost_build::Config::new().compile_fds(files)?;
    Ok(())
}
