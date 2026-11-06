#!/bin/sh
# Runs inside the docker/windows image (started by `./build.sh --target windows`): builds the NSIS
# installer and mau-cli.exe for x86_64-pc-windows-gnu from the repository mounted read-only at /src,
# and leaves them in /out. The target directory and the cargo registry are volumes (/target,
# /cargo-home), so only the first build compiles everything.
set -eu
T=x86_64-pc-windows-gnu
# a copy of rust/ without its target directory: tauri writes into the tree (rust/mau-gui/gen)
rm -rf /work/rust
mkdir -p /work/rust
tar -C /src/rust --exclude=./target --exclude=./mau-gui/gen -cf - . | tar -C /work/rust -xf -
cd /work/rust/mau-gui
# the target volume persists, so installers of older versions would be copied out too
rm -f /target/$T/release/bundle/nsis/*-setup.exe
cargo tauri build --target $T --bundles nsis
cp /target/$T/release/bundle/nsis/*-setup.exe /out/
if [ "${1:-}" != --no-cli ]; then
  cd /work/rust
  cargo build --release -p mau-cli --target $T
  cp /target/$T/release/mau-cli.exe /out/
fi
