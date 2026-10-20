# Repository rules

- This library provides faithful Rust bindings to the official upstream PyroWave library. Preserve upstream behavior; do not introduce behavior changes or workarounds in the bindings.
- Fixes for upstream defects that the pinned revision has not fixed live in `pyrowave-sys/patches/`, one patch file and one commit per fix, applied by the default `native-fixes` feature. Offer each fix upstream and drop it when the pin includes it.
- Use Conventional Commits.
- Do not write code comments unless they explain a non-obvious hack, fix, or detail that is not apparent from the code.
