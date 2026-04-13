fn main() {
    println!("cargo:rustc-link-lib=fmd_adm");
    println!("cargo:rustc-link-search=native=/usr/lib/fm/amd64");
}
