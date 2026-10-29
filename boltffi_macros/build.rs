/// Gives the macros an `OUT_DIR`, which Cargo places inside the build directory of the
/// build compiling them, the directory where `boltffi generate` keeps the record store.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
