// Re-embed the migrations when files are added to or changed in `migrations/`.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
