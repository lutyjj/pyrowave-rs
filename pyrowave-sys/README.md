# PyroWave native bindings

This crate builds the official PyroWave C++ codec and generates bindings for its
C API. [`pyrowave`](../README.md) owns the Rust resource lifetimes.
Use this crate directly for GPU decoder output, device sharing and advanced
synchronization; raw handles cannot extend an existing safe-wrapper codec.
Distribution uses Git checkouts or path dependencies; registry publishing is disabled.

`upstream/pyrowave` and `upstream/Granite` are pinned Git submodules. Initialize
those and the two required Granite dependencies from the repository root:

```sh
git submodule update --init pyrowave-sys/upstream/pyrowave pyrowave-sys/upstream/Granite
git -C pyrowave-sys/upstream/Granite submodule update --init third_party/volk third_party/khronos/vulkan-headers
```

CMake copies the sources into its build directory and applies `patches/*.patch`.
The source submodules stay clean. The native build needs no network after initialization.
The build requires CMake, a C++ compiler, `patch` and libclang. Exported or vendored
sources can build without Git metadata. `UPSTREAM_REVISIONS` records the source
pins and supplies the generated vendor manifest, `UPSTREAM_REVISION` and
`BITSTREAM_ID`. Git checkouts verify those pins against the submodules. To update
the native source, update the submodule Git links and `UPSTREAM_REVISIONS` together,
then validate the build. Preserve all upstream license notices.

The local patches pool encoder buffers, size 4:4:4 payload storage, repair decoder
record validation and scaled-encoding cleanup and precision changes, allow the
NVIDIA modifier import workaround, and add
`pyrowave_image_create_owned_fd`. That extension consumes an imported DMA-BUF
descriptor on every outcome, tracking consumption at successful Vulkan memory
allocation. The standard `pyrowave_image_create` entry point keeps upstream
ownership semantics. These patches do not change the bitstream format.
Adapted patches carry [third-party notices](THIRD-PARTY-NOTICES).

See the [upstream bitstream specification](https://github.com/Themaister/pyrowave/blob/186f0393b77f7755953b5ecde994bb1cec2e4155/bitstream/bitstream.md)
and [C API](https://github.com/Themaister/pyrowave/blob/186f0393b77f7755953b5ecde994bb1cec2e4155/pyrowave.h).

## Container build

After initializing submodules, run `docker build -t pyrowave-rs-build .` from the
repository root. The image compiles both crates and native test tools, and runs
the tests that do not require a GPU. Sources and build outputs stay in the image.

## Native validation

From the repository root, build the official tools against the same patched
codec used by the Rust crate:

```sh
cargo test --workspace --all-features -- --ignored --nocapture
cmake -S pyrowave-sys -B target/native-tools -DCMAKE_BUILD_TYPE=Release
cmake --build target/native-tools --target pyrowave-c-test pyrowave-seam-test pyrowave-decode --parallel 8
target/native-tools/pyrowave-c-test
target/native-tools/pyrowave-seam-test
target/native-tools/pyrowave-decode capture.pyrowave capture.y4m
```

These tools require a compatible Vulkan GPU. The upstream C test checks 4K
roundtrips, rate budgets, decoded image quality, both chroma layouts, partial-frame
readiness and invalid arguments. The decoder accepts the upstream `.pyrowave`
container. Verify output frame count and dimensions: it can return success after
malformed input.
