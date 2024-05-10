MODULE_DIR = ./target/wasm32-unknown-unknown/
WASM_OPT_FLAGS = -Oz --zero-filled-memory --strip-producers --strip-debug

debug:
	cargo build --target=wasm32-unknown-unknown

profile-size:
	cargo build --profile=profile-size --target=wasm32-unknown-unknown
	wasm-opt $(WASM_OPT_FLAGS) -g -o $(MODULE_DIR)/profile-size/skylite_compress.wasm $(MODULE_DIR)/profile-size/skylite_compress.wasm

release:
	cargo build --release --target=wasm32-unknown-unknown
	wasm-opt $(WASM_OPT_FLAGS) -o $(MODULE_DIR)/release/skylite_compress.wasm $(MODULE_DIR)/release/skylite_compress.wasm
