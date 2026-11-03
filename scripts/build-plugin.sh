#!/bin/sh
# Build the engine for Linux and Windows and put it into the plugin.
# Run in the development container:  docker compose run --rm dev scripts/build-plugin.sh
set -eu
cd "$(dirname "$0")/../engine"

for target in x86_64-unknown-linux-gnu x86_64-pc-windows-gnu; do
  cargo build --release --target "$target"
  out="../plugin/engine/$target"
  mkdir -p "$out"
  case "$target" in
    *windows*) cp "$CARGO_TARGET_DIR/$target/release/agentproof.exe" "$out/" ;;
    *) cp "$CARGO_TARGET_DIR/$target/release/agentproof" "$out/" ;;
  esac
  echo "Built $out"
done
