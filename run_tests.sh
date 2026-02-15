#!/bin/bash
set -e

echo "Running unit tests..."
cargo test

echo "Running run-tsx example with timeout..."
cargo run --example run-tsx -- --timeout 3 examples/example.tsx
cargo run --example run-tsx -- --timeout 3 examples/login_form.tsx

echo "All tests passed!"
