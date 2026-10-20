# pyrowave-rs

Thin Rust bindings to the official [PyroWave](https://github.com/Themaister/pyrowave)
1.0 Vulkan codec, using its C++ implementation and shaders.

Building requires Linux, Rust 1.87+, CMake, a C++ compiler and libclang.
Encoding and decoding require a compatible Vulkan GPU.
Initialize the [upstream submodules](pyrowave-sys/README.md) before building.

```toml
[dependencies]
pyrowave = { path = "../pyrowave-rs" }
```

Enable `features = ["dmabuf"]` for encoding imported GPU RGB images.
Create images through `sys` and transfer ownership with `ImportedImage::from_raw()`.
Run `cargo run --example roundtrip` to check encoding and decoding.

Bindings for the complete C API are generated in the `pyrowave-sys` crate and
re-exported as `pyrowave::sys`. The Rust wrappers handle synchronous encoding and
decoding of eight-bit CPU planes, imported RGB encoding in full or narrow range,
queue type and global priority, performance reports and upstream's bitrate model.
Use `sys` for native frame contexts, higher precision CPU planes, GPU decoder output,
GPU selection, inherited devices and synchronization.
`Decoder::push_packet()` is unsafe: callers must supply valid upstream codec records.
See the [API surface and native update guide](pyrowave-sys/README.md).

Use `as_raw()` to borrow a wrapper's native handle and `with_native()` to run raw
native calls that exclude every wrapper call. `Device::from_raw()` takes ownership of a device
created with upstream's advanced device creation functions. Raw callers must follow
upstream's rules for resources, configuration and GPU completion.

`BITSTREAM_VERSION` identifies the frozen v1 packet format. `UPSTREAM_REVISION`
identifies the native source revision, and `BITSTREAM_ID` is its short fingerprint.
The fingerprint changes on every upstream update, even when the packet format
stays the same. Verify interoperability before accepting a different implementation
revision in an existing protocol.

[MIT](LICENSE).
Upstream submodules retain their own licenses.
