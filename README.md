# pyrowave-rs

Thin Rust bindings to the official [PyroWave](https://github.com/Themaister/pyrowave)
1.0 Vulkan codec, using its C++ implementation and shaders.

Requires Linux, Rust 1.87+, `patch`, CMake, a C++ compiler and libclang; running codecs
requires a compatible Vulkan GPU.
Initialize the [upstream submodules](pyrowave-sys/README.md) before building.

```toml
[dependencies]
pyrowave = { path = "../pyrowave-rs" }
```

Enable `features = ["dmabuf"]` for DMA-BUF imports and GPU RGB encoding.
Run `cargo run --example roundtrip` to check encoding and decoding.

The complete C API is generated as `pyrowave::sys`, also available through the
`pyrowave-sys` crate. The ownership wrappers provide synchronous eight-bit CPU
planes, imported RGB encoding, performance reports and upstream's bitrate model.
Native frame contexts, higher precision CPU planes, GPU decoder output, custom
devices, queue priorities and synchronization remain available through `sys`.
See the [API surface and native update guide](pyrowave-sys/README.md).

Use `as_raw()` to borrow a wrapper's native handle and `with_native()` to serialize
native calls with the wrappers. `Device::from_raw()` takes ownership of a device
created with upstream's advanced device creation functions. Raw callers retain
responsibility for upstream's resource, configuration and GPU completion contracts.

`BITSTREAM_VERSION` identifies the frozen v1 packet format. `UPSTREAM_REVISION`
identifies the native source; `BITSTREAM_ID` is its short fingerprint, so it
changes even for updates that preserve the packet format. Verify interoperability
before accepting a different implementation revision in an existing protocol.

[MIT](LICENSE), with [third-party notices](pyrowave-sys/THIRD-PARTY-NOTICES).
Upstream submodules retain their own licenses.
