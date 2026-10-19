# Tareas comunes del proyecto. En Windows sin `make`, usar scripts/lint.ps1.
C_SOURCES := $(shell find crates/runtime-c -name '*.c' -o -name '*.h' 2>/dev/null)

.PHONY: all build test fmt fmt-check clippy c-fmt-check lint coverage clean

all: build

build:
	cargo build --workspace --all-targets

test:
	cargo test --workspace

fmt:
	cargo fmt --all
	$(if $(C_SOURCES),clang-format -i $(C_SOURCES))

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

c-fmt-check:
	$(if $(C_SOURCES),clang-format --dry-run --Werror $(C_SOURCES))

# El frontend (web/) se añade a este objetivo en T7.7 (eslint + prettier).
lint: fmt-check clippy c-fmt-check

coverage:
	cargo llvm-cov --workspace --lcov --output-path lcov.info

clean:
	cargo clean
