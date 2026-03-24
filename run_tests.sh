#!/bin/bash
set -e

echo "Running unit tests..."
cargo test

echo "Running run-tsx examples with timeout..."
#cargo run -p matterstream --features compiler --example run-tsx -- --timeout 3 crates/matterstream/examples/example.tsx
cargo run -p matterstream --features compiler --example run-tsx -- --timeout 3 crates/matterstream/examples/login_form.tsx

echo "All tests passed!"
