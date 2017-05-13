# emojistats-bot

A Discord bot that provides statistics on emoji usage.

## Build notes

As of 13 May 2017, [discord](https://crates.io/crates/discord/0.8.0) relies on [websocket ^0.17](https://crates.io/crates/websocket/0.17.1), which in turn relies on [openssl ^0.7.6](https://crates.io/crates/websocket/0.17.1). If you run into difficulties with compiling [rust-openssl v0.7.x](https://github.com/sfackler/rust-openssl/blob/b8fb29db5c246175a096260eacca38180cd77dd0/README.md), try:

- `cargo clean`
- `export OPENSSL_INCLUDE_DIR=` (OpenSSL 1.0 include directory)
- `export OPENSSL_LIB_DIR=` (OpenSSL 1.0 lib directory)
- `cargo build`

The following values worked for me on Arch Linux:

```bash
export OPENSSL_INCLUDE_DIR=/usr/include/openssl-1.0
export OPENSSL_LIB_DIR=/usr/lib/openssl-1.0
```
