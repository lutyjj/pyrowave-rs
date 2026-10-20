# PyroWave native bindings

This crate builds the official PyroWave 1.0 C++ codec and generates bindings for
its complete C API. [`pyrowave`](../README.md) manages resource lifetimes in Rust and
re-exports these bindings as `pyrowave::sys`.
Use a Git checkout or path dependency; registry publishing is disabled.

## API surface

Bindgen generates bindings from `pyrowave.h`, so the full native API is available
even when the wrapper has no convenience method. A generated test takes the
address of every function in upstream's `pyrowave-shared.def` and checks that
each Rust declaration links to its native symbol. This test runs without a GPU.

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

`Device`, `Encoder`, `Decoder` and `ImportedImage` expose borrowed `as_raw()`
handles. Keep their owners alive and do not destroy borrowed handles. Run raw
calls inside `pyrowave::with_native()` whenever wrappers are used in the same
process. `with_native()` excludes every wrapper call, as device creation and
destruction do: creation rewrites Vulkan dispatch state shared by all devices.
Other wrapper calls on separate devices run concurrently. The closure must not call wrapper
methods because they acquire the same lock.

Raw calls must preserve the wrapper's configured format. Finish work that uses
borrowed resources before reusing them, and clear device command-buffer overrides
before returning to wrapper methods. Your application owns any Vulkan handles
inherited by a native device. Keep those handles alive until all wrappers using
that device have dropped.

## Build and upstream updates

`upstream/pyrowave` and `upstream/Granite` are pinned Git submodules. Initialize
those and the two required Granite dependencies from the repository root:

```sh
git submodule update --init pyrowave-sys/upstream/pyrowave pyrowave-sys/upstream/Granite
git -C pyrowave-sys/upstream/Granite submodule update --init third_party/volk third_party/khronos/vulkan-headers
```

CMake assembles unmodified sources in its build directory. Native code and
compiled shaders retain upstream behavior, including defects and limitations.
After initialization, the native build needs no network. Building requires
CMake, a C++ compiler and libclang.
Exported or vendored sources can build without Git metadata.

`UPSTREAM_REVISIONS` records the source pins used for the generated vendor
manifest, `UPSTREAM_REVISION` and `BITSTREAM_ID`. Builds from Git checkouts verify
those pins against the submodules. For an upstream refresh, change the submodule
Git links and `UPSTREAM_REVISIONS` together, then validate the build. Preserve all
upstream license notices.

`quality.c` exposes upstream's bitrate model, a static function in
`eval-results/pyrowave_regression_results.h`.

See the [upstream bitstream specification](https://github.com/Themaister/pyrowave/blob/21f099d24df0ab0e4c10c3288c924b6b1717e581/bitstream/bitstream.md)
and [C API](https://github.com/Themaister/pyrowave/blob/21f099d24df0ab0e4c10c3288c924b6b1717e581/pyrowave.h).
API, wrapper, build and native export checks must pass after each refresh.
GPU roundtrips and interop tests establish runtime behavior on the tested driver;
a successful build alone does not establish that behavior or upstream performance.

## Container build

After initializing submodules, run `docker build -t pyrowave-rs-build .` from the
repository root. The image compiles both crates and native test tools, and runs
the tests that do not require a GPU. Sources and build outputs stay in the image.

## Native validation

From the repository root, build the official tools against the same native
sources as the Rust crate:

```sh
cmake -S pyrowave-sys -B target/native-tools -DCMAKE_BUILD_TYPE=Release
cmake --build target/native-tools --target pyrowave-c-test pyrowave-c-interop-test pyrowave-device-validation pyrowave-decode --parallel 8
```

On a compatible Vulkan GPU, run:

```sh
cargo test --workspace --all-features -- --ignored --nocapture
target/native-tools/pyrowave-c-test
target/native-tools/pyrowave-c-interop-test
target/native-tools/pyrowave-device-validation --roundtrip
target/native-tools/pyrowave-decode capture.pyrowave capture.y4m
```

The upstream C test checks 4K roundtrips, rate budgets and decoded image quality.
It covers both chroma layouts, higher precision CPU planes, frame contexts,
asynchronous readback, partial-frame readiness and invalid arguments. The interop
test exercises shared Vulkan devices, imported resources and synchronization.

The decoder accepts upstream's versioned `PWV1` disk container. Verify output
frame count and dimensions: malformed input can still return success.
