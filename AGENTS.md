# Repository rules

- This library provides faithful Rust bindings to the official upstream PyroWave library. Preserve upstream behavior; do not fix upstream defects, apply native patches, or introduce behavior changes or workarounds in the bindings.
- Exception for the `patches` branch only: it carries native fixes in `pyrowave-sys/patches/` for upstream defects that the pinned upstream revision has not fixed. Keep one commit and one patch file per fix, and drop a fix when upstream includes it.
- Use Conventional Commits.
- Do not write code comments unless they explain a non-obvious hack, fix, or detail that is not apparent from the code.
