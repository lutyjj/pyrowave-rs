# pyrowave-rs

Thin Rust bindings to the official [PyroWave](https://github.com/Themaister/pyrowave)
Vulkan codec, using its C++ implementation.

Requires Linux, Rust 1.87+, `patch`, CMake, a C++ compiler and libclang; running codecs
requires a compatible Vulkan GPU.
Initialize the [upstream submodules](pyrowave-sys/README.md) before building.

```toml
[dependencies]
pyrowave = { path = "../pyrowave-rs" }
```

Enable `features = ["dmabuf"]` for DMA-BUF imports and GPU RGB encoding.
Run `cargo run --example roundtrip` to check encoding and decoding.

[MIT](LICENSE), with [third-party notices](pyrowave-sys/THIRD-PARTY-NOTICES).
Upstream submodules retain their own licenses.
