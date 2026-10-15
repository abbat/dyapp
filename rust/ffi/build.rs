//! Generate the empty UniFFI namespace scaffold used alongside proc-macro exports.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    uniffi::generate_scaffolding("src/dyapp.udl")?;
    Ok(())
}
