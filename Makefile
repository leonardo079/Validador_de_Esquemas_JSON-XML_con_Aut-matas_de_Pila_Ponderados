# Tareas comunes del proyecto. En Windows sin `make`, usar scripts/lint.ps1.
C_SOURCES := $(shell find crates/runtime-c -name '*.c' -o -name '*.h' 2>/dev/null)

.PHONY: all build test fmt fmt-check clippy c-fmt-check lint coverage docs docs-check clean

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

# Comprueba que los artefactos deterministas de `make docs` coinciden con los
# versionados: los `.dot` de la Fase 1 y las tablas `ll1-*.md` de la Fase 2.
# Los regenera en `target/` y los compara con los del árbol de trabajo, así
# que no depende de git ni del estado del índice. Los `.svg` quedan fuera: la
# salida de `dot -Tsvg` incluye la versión de Graphviz y coordenadas que
# dependen de las métricas de fuentes del sistema, así que no es reproducible
# entre máquinas; se regeneran con `make docs` a partir del `.dot` verificado.
# `json-izquierda` no es LL(1) a propósito (ll1.py sale con 1), por eso no se
# exige su código de salida, pero sí su contenido.
DOCS_TMP := target/docs-check
docs-check:
	@rm -rf $(DOCS_TMP) && mkdir -p $(DOCS_TMP)
	@fallo=0; \
	for a in utf8 json xml; do \
		cargo run -q -p cli -- dot $$a > $(DOCS_TMP)/afd-$$a.dot || exit 1; \
		diff -u docs/fase1/afd-$$a.dot $(DOCS_TMP)/afd-$$a.dot || fallo=1; \
	done; \
	for g in json xml esquema; do \
		python3 scripts/ll1.py docs/fase2/$$g.ll1 > $(DOCS_TMP)/ll1-$$g.md || exit 1; \
		diff -u docs/fase2/ll1-$$g.md $(DOCS_TMP)/ll1-$$g.md || fallo=1; \
	done; \
	python3 scripts/ll1.py docs/fase2/json-izquierda.ll1 > $(DOCS_TMP)/ll1-json-izquierda.md; \
	diff -u docs/fase2/ll1-json-izquierda.md $(DOCS_TMP)/ll1-json-izquierda.md || fallo=1; \
	if [ $$fallo -ne 0 ]; then \
		echo "docs/ desincronizado con el código: ejecutar 'make docs' y versionar el resultado"; \
		exit 1; \
	fi; \
	echo "docs-check: .dot de la Fase 1 y ll1-*.md de la Fase 2 sincronizados"

clean:
	cargo clean
