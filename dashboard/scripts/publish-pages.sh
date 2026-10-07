#!/usr/bin/env bash
set -euo pipefail

# Preserve the framework's content hashes by publishing after all optimization.
cargo run -p cloud_dashboard --bin manage -- buildstatic \
    --package cloud_dashboard --pages-dir dist-wasm \
    --pages-entry cloud_dashboard.js --pages-document index.html
