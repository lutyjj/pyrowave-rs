# pyrowave-rs

Rust bindings to the official [PyroWave](https://github.com/Themaister/pyrowave)
Vulkan codec. The crate builds upstream's C++ implementation and shaders at a pinned
revision. The default `native-fixes` feature applies the fixes in
[`pyrowave-sys/patches`](pyrowave-sys/patches) for upstream defects that revision still
has; with `default-features = false`, the crate builds upstream unmodified, defects
included.

Building requires Linux, the Rust version in `Cargo.toml`, CMake, a C++ compiler,
libclang, the [upstream submodules](pyrowave-sys/README.md#build-and-upstream-updates)
and, for `native-fixes`, GNU `patch`.
Encoding and decoding require a Vulkan GPU.

```toml
[dependencies]
pyrowave = { git = "https://github.com/lutyjj/pyrowave-rs", rev = "<commit>" }
```

The wrappers handle synchronous encoding and decoding of eight-bit CPU planes,
imported RGB encoding (feature `dmabuf`), queue type and global priority,
performance reports and upstream's bitrate model. `pyrowave::sys` re-exports the
complete C API; the [native bindings guide](pyrowave-sys/README.md) maps each upstream
capability to its Rust access and sets the rules for mixing raw calls with wrappers.
`Decoder::push_packet()` is unsafe: callers must supply valid upstream codec records.
`cargo run --example roundtrip` encodes and decodes one frame.

`BITSTREAM_VERSION` identifies the packet format version. `UPSTREAM_REVISION` is the
pinned upstream revision and `BITSTREAM_ID` its first eight hex digits. Both change on
every upstream update, even when the packet format stays the same, and neither
reflects the local patches. Verify interoperability before accepting a different
`BITSTREAM_ID` in an existing protocol.

[MIT](LICENSE). Upstream submodules keep their own licenses.
