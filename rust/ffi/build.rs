fn main() {
    uniffi::generate_scaffolding("src/dyapp.udl").expect("generate UniFFI namespace scaffold");
}
