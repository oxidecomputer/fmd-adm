fn main() {
    println!("cargo:rustc-link-lib=fmd_adm");
    println!("cargo:rustc-link-search=native=/usr/lib/fm/amd64");
    // Emit metadata so downstream crates can configure RPATH for the
    // non-standard /usr/lib/fm/amd64 location. Cargo exposes this to
    // direct dependents as the env var DEP_FMD_ADM_LIBDIRS.
    println!("cargo:LIBDIRS=/usr/lib/fm/amd64");
}
