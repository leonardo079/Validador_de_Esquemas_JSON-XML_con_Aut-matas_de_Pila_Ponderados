# Tareas comunes del proyecto. En Windows sin `make`, usar scripts/lint.ps1.
C_SOURCES := $(shell find crates/runtime-c -name '*.c' -o -name '*.h' 2>/dev/null)

.PHONY: all build test fmt fmt-check clippy c-fmt-check lint coverage docs clean

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

# Diagramas y tablas generados desde el código (requiere Graphviz).
docs:
	for a in utf8 json xml; do \
		cargo run -q -p cli -- dot $$a > docs/fase1/afd-$$a.dot && \
		dot -Tsvg docs/fase1/afd-$$a.dot -o docs/fase1/afd-$$a.svg; \
	done
	python3 scripts/ll1.py docs/fase2/json.ll1 > docs/fase2/ll1-json.md
	python3 scripts/ll1.py docs/fase2/xml.ll1 > docs/fase2/ll1-xml.md
	python3 scripts/ll1.py docs/fase2/esquema.ll1 > docs/fase2/ll1-esquema.md
	python3 scripts/ll1.py docs/fase2/json-izquierda.ll1 > docs/fase2/ll1-json-izquierda.md || true

clean:
	cargo clean
