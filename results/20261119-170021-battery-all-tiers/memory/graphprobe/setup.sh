#!/bin/sh
# Copy automerge 0.12.0 from the cargo registry and apply the memory-report patch.
set -e
cd "$(dirname "$0")"
src=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/automerge-0.12.0 | head -1)
rm -rf am-patched && cp -r "$src" am-patched
patch -p1 -d am-patched < automerge-0.12.0-memory-report.patch
