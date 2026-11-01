# Font handlings utils

Simple Rust crate for loading fonts, rasterizing glyphs, generating sdfs and allocating atlas tiles.

Bindings for the Jai programming language. I don't want to rewrite all these well working Rust crates in Jai at the moment. 


# Building:


For Linux on Linux:
```
cargo build --release
cp target/release/libfontutils.so lib/

```


Cross compile for Windows on Linux:
```
cargo zigbuild --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/fontutils.dll lib/
cp target/x86_64-pc-windows-gnu/release/libfontutils.dll.a lib/fontutils.lib
```