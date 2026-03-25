.PHONY: test test-no-ui test-ui test-ui-soft test-all check-all examples

# Default: run all three test configurations
test-all: test-no-ui test-ui test-ui-soft
	@echo "=== All three test configurations passed ==="

# No UI feature — VM NOPs UI ops
test-no-ui:
	@echo "=== Testing: no ui feature (NOP mode) ==="
	cargo test -p matterstream-vm --no-default-features
	cargo test -p matterstream-common
	cargo test -p matterstream-vm-addressing
	cargo test -p matterstream-packaging
	cargo run -p matterstream --example oid-import-ui -- --timeout 1

# Default features (ui enabled) — VM produces draw commands, no rendering
test-ui:
	@echo "=== Testing: ui feature (draw commands, no rendering) ==="
	cargo test
	cargo run -p matterstream-vm --example package_demo
	cargo run -p matterstream-vm --example oid_import_demo
	cargo run -p matterstream --example oid-import-ui -- --timeout 1

# UI + softbuffer — full rendering pipeline
test-ui-soft:
	@echo "=== Testing: ui + ui-softbuffer (full rendering) ==="
	cargo test -p matterstream-ui-soft
	cargo run -p matterstream --features ui-softbuffer --example oid-import-ui -- --timeout 2

# Check all configurations compile
check-all:
	@echo "=== Checking: no-default-features ==="
	cargo check -p matterstream-vm --no-default-features
	@echo "=== Checking: default features ==="
	cargo check
	@echo "=== Checking: ui-softbuffer ==="
	cargo check -p matterstream --features ui-softbuffer
	@echo "=== All configurations compile ==="

# Run all examples
examples:
	cargo run -p matterstream-vm --example package_demo
	cargo run -p matterstream-vm --example oid_import_demo
	cargo run -p matterstream --example oid-import-ui -- --timeout 1
	cargo run -p matterstream --features ui-softbuffer --example oid-import-ui -- --timeout 2

# Alias
test: test-all
