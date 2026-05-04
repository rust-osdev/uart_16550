fn main() {
    let linker_script = "link.ld";

    println!("cargo:rerun-if-changed={linker_script}");
    println!("cargo:rustc-link-arg=-T{linker_script}");
}
