fn main() {
    // `cfg!(target_os)` in a build script refers to the host, so check the actual target instead.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libfontutils.so");
    }
}
