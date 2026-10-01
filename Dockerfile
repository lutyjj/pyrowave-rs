FROM rust:1.87-bookworm@sha256:251cec8da4689d180f124ef00024c2f83f79d9bf984e43c180a598119e326b84

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake libclang-dev patch \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work
COPY . .

RUN cargo test --workspace --all-features --locked \
    && cmake -S pyrowave-sys -B target/native-tools -DCMAKE_BUILD_TYPE=Release \
    && cmake --build target/native-tools --target pyrowave-c-test pyrowave-seam-test pyrowave-decode --parallel 8
