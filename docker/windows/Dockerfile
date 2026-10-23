# Build image for `./build.sh --target windows` on Linux or a Mac: cross-compiles the Windows
# installer (NSIS) and fml-cli.exe (x86_64-pc-windows-gnu, MinGW) in a container, so no Windows
# machine is needed. Nothing here is Windows software; it runs natively (no --platform).
# Adapted from ~/code/pampas/docker/windows: fml has no Node step and no crypto library that
# needs NASM or CMake, so those are left out.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates git file xz-utils mingw-w64 nsis \
    && update-alternatives --set x86_64-w64-mingw32-gcc /usr/bin/x86_64-w64-mingw32-gcc-posix \
    && update-alternatives --set x86_64-w64-mingw32-g++ /usr/bin/x86_64-w64-mingw32-g++-posix \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable -t x86_64-pc-windows-gnu \
    && cargo install tauri-cli --version '^2' --locked
WORKDIR /work
