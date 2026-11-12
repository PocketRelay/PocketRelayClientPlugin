```sh
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
cargo install --locked cargo-xwin
cargo +nightly xwin build --target i686-win7-windows-msvc --release
```
