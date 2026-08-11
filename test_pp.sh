#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

echo "=== Building ==="
cargo build --release --example test_pp

echo ""
echo "=== Running PP Calculation ==="
./target/release/examples/test_pp
