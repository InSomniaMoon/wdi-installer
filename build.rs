// libwdi-sys (0.1.5) only declares `cargo:rustc-link-lib=shell32`/`ole32`
// in its own build.rs, but the vendored C sources it compiles (libwdi.c,
// libwdi_dlg.c, pki.c) also call into user32 (windows/messages), advapi32
// (security tokens, registry, CryptoAPI) and setupapi (device
// enumeration). Without linking those three standard Windows system
// libraries too (part of every Windows SDK, nothing to download), the
// link step fails with `unresolved external symbol` errors.
fn main() {
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=advapi32");
    println!("cargo:rustc-link-lib=setupapi");
}
