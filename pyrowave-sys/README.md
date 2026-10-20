# PyroWave native bindings

`pyrowave-sys` builds the official PyroWave C++ codec and generates bindings for
its complete C API. [`pyrowave`](../README.md) re-exports them as `pyrowave::sys` and
manages resource lifetimes. Depend on a Git checkout or a path; registry publishing is
disabled.

## API surface

Bindgen generates the bindings from `pyrowave.h`. A generated test checks, without a
GPU, that every function in upstream's `pyrowave-shared.def` links.

| Upstream capability | Rust access |
| --- | --- |
| API version | `api_version()` or `sys` |
| Default device and performance reports | `Device` |
| GPU selection, inherited devices, queue priority, queue type and command buffers | `sys`; `Device::with_global_priority()` and `Device::set_queue_type()` cover the default GPU; adopt an owned native device with `Device::from_raw()` |
| External semaphore import/export, CPU waits/signals and GPU synchronization | `sys` |
| External images and image views | `sys`; `ImportedImage::from_raw()` owns an imported native image |
| Planar CPU input, including NV12 and 8-, 10- and 16-bit samples | `sys`; `Encoder::encode_cpu()` handles eight-bit planar input |
| GPU planes, RGB conversion, scaling, crop, dither and full/narrow YCbCr range | `sys`; `Encoder::encode_image()` handles imported RGB in full or narrow range with an explicit chroma midpoint |
| Frame contexts and asynchronous readback | `sys` |
| Packet boundaries, padding, critical packets, active blocks and mapped bitstreams | `sys`; the wrapper returns one contiguous frame and counts active blocks |
| Packet ingestion, readiness and partial frames with sideband information | `sys`; `Decoder` handles ingestion and readiness |
| GPU decoder output and synchronous/asynchronous CPU output at supported depths | `sys`; `Decoder::decode_cpu()` reads eight-bit planes synchronously |
| Per-stage timings and upstream's bitrate model | `Device::performance_report()` and `estimate_bitrate()` |

`Device`, `Encoder`, `Decoder` and `ImportedImage` lend their native handles through
`as_raw()`. Keep the owner alive and do not destroy a borrowed handle. When a process
mixes raw calls with wrappers, run the raw calls inside `pyrowave::with_native()`. It
excludes every wrapper call, as device creation and destruction do, because creation
rewrites Vulkan dispatch state shared by all devices. Wrapper calls on separate
devices otherwise run concurrently. The closure must not call wrapper methods, which
take the same lock.

Raw calls must keep the wrapper's configured format, finish GPU work on borrowed
resources before reusing them, and clear device command-buffer overrides before
returning to wrapper methods. The application owns any Vulkan handles that a native
device inherits and keeps them alive until every wrapper using that device has dropped.

## Build and upstream updates

`upstream/pyrowave` and `upstream/Granite` are pinned submodules. From the repository
root, initialize them and the two Granite dependencies the build uses:

```sh
git submodule update --init pyrowave-sys/upstream/pyrowave pyrowave-sys/upstream/Granite
git -C pyrowave-sys/upstream/Granite submodule update --init third_party/volk third_party/khronos/vulkan-headers
```

After that the build needs no network. Exported or vendored sources build without Git
metadata.

CMake copies the upstream sources into the build directory. With the default
`native-fixes` feature (CMake option `PYROWAVE_NATIVE_FIXES`), it applies
`patches/*.patch` to that copy in file-name order; the submodules stay unmodified. Each
patch fixes one upstream defect, names it in its header and has its own commit. The build fails
when a patch does not apply, including when upstream already contains the fix. The
build's `vendor/pyrowave/VENDOR.txt` lists the pins and the applied patches.

`UPSTREAM_REVISIONS` lists the pins, and builds from a Git checkout fail when the
submodules differ from it. To refresh upstream, update the submodules and
`UPSTREAM_REVISIONS` together, drop the patches the new pin includes, and run the
validation below. Preserve upstream license notices.

`quality.c` exposes upstream's bitrate model, a static function in
`eval-results/pyrowave_regression_results.h`. Upstream documents the bitstream in
`upstream/pyrowave/bitstream/bitstream.md` and the C API in `upstream/pyrowave/pyrowave.h`.

## Validation

After initializing the submodules, `docker build -t pyrowave-rs-build .` from the
repository root compiles both crates and the native test tools and runs the tests that
need no GPU.

On a Vulkan GPU, build upstream's tools from the same sources as the crate and run them
with the GPU tests. Add `-DPYROWAVE_NATIVE_FIXES=OFF` to the first command to build them
from unmodified upstream.

```sh
cmake -S pyrowave-sys -B target/native-tools -DCMAKE_BUILD_TYPE=Release
cmake --build target/native-tools --target pyrowave-c-test pyrowave-c-interop-test pyrowave-device-validation pyrowave-decode --parallel 8
cargo test --workspace --all-features -- --ignored --nocapture
target/native-tools/pyrowave-c-test
target/native-tools/pyrowave-c-interop-test
target/native-tools/pyrowave-device-validation --roundtrip
target/native-tools/pyrowave-decode capture.pyrowave capture.y4m
```

Only these GPU runs check runtime behavior, and only on the driver they ran on.
`pyrowave-decode` reads upstream's `PWV1` container and can return success on
malformed input, so check its frame count and dimensions.
