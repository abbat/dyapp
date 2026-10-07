fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto");
    let files = protox::compile(["node.proto"], ["../../proto"])?;
    prost_build::Config::new()
        .extern_path(".dyapp.identity", "::dyapp_identity")
        .compile_fds(files)?;
    Ok(())
}
