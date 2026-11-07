fn main() {
    // The catalog is embedded with include_dir!; rebuild whenever anything in it changes.
    println!("cargo:rerun-if-changed=catalog");
}
