# Tareas comunes del proyecto. En Windows sin `make`, usar scripts/lint.ps1.
C_SOURCES := $(shell find crates/runtime-c -name '*.c' -o -name '*.h' 2>/dev/null)

.PHONY: all build test test-baseline-xml test-sandbox fuzz fmt fmt-check clippy c-fmt-check unsafe-check lint coverage docs docs-check doradas clean

all: build

build:
	cargo build --workspace --all-targets

test:
	cargo test --workspace

# Línea base XML (feature `libxml2` de `crates/baseline`, F6.7b): libxml2,
# pkg-config y libclang dentro de Docker, para no pedirlos en la máquina. La
# imagen fija Rust por versión y digest; las dependencias se descargan con
# red en un volumen de caché y las pruebas y clippy corren SIN red, con el
# repositorio en solo lectura y el `target` en otro volumen
# (`docs/fase6/linea-base.md` §7.3). El paso de pruebas corre además sin
# capacidades ni escalada de privilegios. Mantener a la par con
# `scripts/test-baseline-xml.ps1` (Windows sin make), flag por flag.
BASELINE_XML_IMAGE := validador-baseline-xml:1
BASELINE_XML_RUN = docker run --rm -v "$(CURDIR)":/src:ro \
	-v validador-cargo-registry:/usr/local/cargo/registry \
	-v validador-baseline-xml-target:/target -w /src
test-baseline-xml:
	docker build -t $(BASELINE_XML_IMAGE) crates/baseline/docker
	$(BASELINE_XML_RUN) $(BASELINE_XML_IMAGE) cargo fetch --locked
	$(BASELINE_XML_RUN) --network none --cap-drop ALL --security-opt no-new-privileges \
		$(BASELINE_XML_IMAGE) sh scripts/test-baseline-xml.sh

# F7.3 (D68): pruebas del sandbox con contenedores reales (`docs/fase7/sandbox.md`).
# Mantener a la par con `scripts/test-sandbox.ps1`.
test-sandbox:
	docker build -t validador-sandbox:1 sandbox
	cargo test -p server --release --test sandbox -- --ignored --test-threads 1

fmt:
	cargo fmt --all
	$(if $(C_SOURCES),clang-format -i $(C_SOURCES))

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

c-fmt-check:
	$(if $(C_SOURCES),clang-format --dry-run --Werror $(C_SOURCES))

# Invariante `unsafe_code = "deny"` con una única excepción nombrada,
# decidida por el usuario el 16 nov (opción A, docs/fase4/pendientes.md §8):
# el asignador contador de UNSAFE_OK (`GlobalAlloc` es un rasgo `unsafe`).
# Falla si aparece `allow(unsafe_code)`, `unsafe impl`, `unsafe fn`,
# `unsafe extern`, `unsafe trait` o un bloque `unsafe {` en cualquier `.rs`
# fuera de UNSAFE_OK, o si en UNSAFE_OK hay algo más que el `allow`, el
# `unsafe impl GlobalAlloc` y sus tres funciones que delegan en `System`.
# Las líneas de comentario (`//`) no cuentan. Solo `find` y `grep -E`.
UNSAFE_OK := crates/automaton/tests/sin_reservas.rs
UNSAFE_RE := allow\(unsafe_code\)|unsafe[[:space:]]+(impl|fn|extern|trait)|unsafe[[:space:]]*\{
UNSAFE_PERMITIDO := ^[0-9]+:(\#\[allow\(unsafe_code\)\]|unsafe impl GlobalAlloc for Contador \{|    unsafe fn (alloc|dealloc|realloc)\(.*|        unsafe \{ System\.(alloc|dealloc|realloc)\(.*\) \})$$
unsafe-check:
	@fuera=$$(find . -path ./target -prune -o -path '*/node_modules' -prune -o \
		-name '*.rs' ! -path './$(UNSAFE_OK)' -print \
		| xargs grep -nHE '$(UNSAFE_RE)' /dev/null | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'); \
	dentro=$$(grep -nE '$(UNSAFE_RE)' $(UNSAFE_OK) | grep -vE '^[0-9]+:[[:space:]]*//' \
		| grep -vE '$(UNSAFE_PERMITIDO)' | sed 's|^|$(UNSAFE_OK):|'); \
	n=$$(grep -cE '^unsafe impl GlobalAlloc for Contador \{$$' $(UNSAFE_OK)); \
	if [ -n "$$fuera$$dentro" ] || [ "$$n" -ne 1 ]; then \
		[ -n "$$fuera$$dentro" ] && printf '%s\n' "$$fuera" "$$dentro" | grep -v '^$$'; \
		[ "$$n" -ne 1 ] && echo "$(UNSAFE_OK): se esperaba un solo 'unsafe impl GlobalAlloc' y hay $$n"; \
		echo "unsafe-check: unsafe fuera de la única excepción registrada ($(UNSAFE_OK), docs/fase4/pendientes.md §8)"; \
		exit 1; \
	fi; \
	echo "unsafe-check: sin unsafe salvo la excepción registrada ($(UNSAFE_OK), docs/fase4/pendientes.md §8)"

# F6.4 (docs/fase6/fuzzing.md): cargo-fuzz con nightly, SEG segundos por
# objetivo, y lo encontrado por la prueba diferencial con el C. No es parte
# de `test` (necesita nightly y no es determinista en tiempo).
SEG ?= 60
fuzz:
	sh scripts/fuzz.sh $(SEG)

# El frontend (web/) se añade a este objetivo en T7.7 (eslint + prettier).
lint: fmt-check clippy c-fmt-check unsafe-check

coverage:
	cargo llvm-cov --workspace --lcov --output-path lcov.info

# Diagramas y tablas generados desde el código (requiere Graphviz).
# IR_DOT: esquemas cuya IR se dibuja en docs/fase3/ir-<nombre>.dot (Fase 3,
# `schemac ir --dot`); el de ejemplo tiene recursión productiva. IR_DOT_XSD:
# lo mismo con los XSD de tests/schemas/xsd/parse-ok (tarea 5.4).
IR_DOT := arbol-recursivo
IR_DOT_XSD := pedido
# AUT_DOT: esquemas de tests/schemas/parse-ok cuyo autómata M (Fase 4,
# `schemac automaton --dot`) se dibuja en docs/fase4/automata-<nombre>.dot;
# `objeto-cerrado` es la dorada contada a mano en docs/fase4/automata.md §8.
AUT_DOT := objeto-cerrado
# AUT_DOT_XSD: lo mismo con los XSD de tests/schemas/xsd/parse-ok (F4.1c);
# `valid-xml-pedido-01` es el esquema del caso del corpus del mismo nombre.
# `-Gnewrank=true`: sin él, `dot` pierde aristas entre clústeres de ese grafo.
AUT_DOT_XSD := valid-xml-pedido-01
# DERIV_DOT: payloads de docs/fase5/ejemplos cuyo árbol de derivación (F5.5,
# `arbol --dot`) se dibuja en docs/fase5/arbol-<archivo>.dot.
DERIV_DOT := pedido.json pedido.xml
docs:
	for a in utf8 json xml; do \
		cargo run -q -p cli -- dot $$a > docs/fase1/afd-$$a.dot && \
		dot -Tsvg docs/fase1/afd-$$a.dot -o docs/fase1/afd-$$a.svg; \
	done
	for e in $(IR_DOT); do \
		cargo run -q -p cli -- ir --dot tests/schemas/parse-ok/$$e.json > docs/fase3/ir-$$e.dot && \
		dot -Tsvg docs/fase3/ir-$$e.dot -o docs/fase3/ir-$$e.svg; \
	done
	for e in $(IR_DOT_XSD); do \
		cargo run -q -p cli -- ir --dot tests/schemas/xsd/parse-ok/$$e.xsd > docs/fase3/ir-$$e.dot && \
		dot -Tsvg docs/fase3/ir-$$e.dot -o docs/fase3/ir-$$e.svg; \
	done
	for e in $(AUT_DOT); do \
		cargo run -q -p cli -- automaton --dot tests/schemas/parse-ok/$$e.json > docs/fase4/automata-$$e.dot && \
		dot -Tsvg docs/fase4/automata-$$e.dot -o docs/fase4/automata-$$e.svg; \
	done
	for e in $(AUT_DOT_XSD); do \
		cargo run -q -p cli -- automaton --dot tests/schemas/xsd/parse-ok/$$e.xsd > docs/fase4/automata-$$e.dot && \
		dot -Tsvg -Gnewrank=true docs/fase4/automata-$$e.dot -o docs/fase4/automata-$$e.svg; \
	done
	for e in $(DERIV_DOT); do \
		cargo run -q -p derivation --bin arbol -- --dot docs/fase5/ejemplos/$$e > docs/fase5/arbol-$$e.dot && \
		dot -Tsvg docs/fase5/arbol-$$e.dot -o docs/fase5/arbol-$$e.svg; \
	done
	python3 scripts/ll1.py docs/fase2/json.ll1 > docs/fase2/ll1-json.md
	python3 scripts/ll1.py docs/fase2/xml.ll1 > docs/fase2/ll1-xml.md
	python3 scripts/ll1.py docs/fase2/esquema.ll1 > docs/fase2/ll1-esquema.md
	python3 scripts/ll1.py docs/fase2/xsd.ll1 > docs/fase2/ll1-xsd.md
	python3 scripts/ll1.py docs/fase2/json-izquierda.ll1 > docs/fase2/ll1-json-izquierda.md || true

# Comprueba que los artefactos deterministas de `make docs` coinciden con los
# versionados: los `.dot` de la Fase 1, las tablas `ll1-*.md` de la Fase 2 y
# los `.dot` de la IR de la Fase 3 (de JSON Schema y de XSD) y los del
# autómata M de la Fase 4.
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
	for e in $(IR_DOT); do \
		cargo run -q -p cli -- ir --dot tests/schemas/parse-ok/$$e.json > $(DOCS_TMP)/ir-$$e.dot || exit 1; \
		diff -u docs/fase3/ir-$$e.dot $(DOCS_TMP)/ir-$$e.dot || fallo=1; \
	done; \
	for e in $(IR_DOT_XSD); do \
		cargo run -q -p cli -- ir --dot tests/schemas/xsd/parse-ok/$$e.xsd > $(DOCS_TMP)/ir-$$e.dot || exit 1; \
		diff -u docs/fase3/ir-$$e.dot $(DOCS_TMP)/ir-$$e.dot || fallo=1; \
	done; \
	for e in $(AUT_DOT); do \
		cargo run -q -p cli -- automaton --dot tests/schemas/parse-ok/$$e.json > $(DOCS_TMP)/automata-$$e.dot || exit 1; \
		diff -u docs/fase4/automata-$$e.dot $(DOCS_TMP)/automata-$$e.dot || fallo=1; \
	done; \
	for e in $(AUT_DOT_XSD); do \
		cargo run -q -p cli -- automaton --dot tests/schemas/xsd/parse-ok/$$e.xsd > $(DOCS_TMP)/automata-$$e.dot || exit 1; \
		diff -u docs/fase4/automata-$$e.dot $(DOCS_TMP)/automata-$$e.dot || fallo=1; \
	done; \
	for e in $(DERIV_DOT); do \
		cargo run -q -p derivation --bin arbol -- --dot docs/fase5/ejemplos/$$e > $(DOCS_TMP)/arbol-$$e.dot || exit 1; \
		diff -u docs/fase5/arbol-$$e.dot $(DOCS_TMP)/arbol-$$e.dot || fallo=1; \
	done; \
	for g in json xml esquema xsd; do \
		python3 scripts/ll1.py docs/fase2/$$g.ll1 > $(DOCS_TMP)/ll1-$$g.md || exit 1; \
		diff -u docs/fase2/ll1-$$g.md $(DOCS_TMP)/ll1-$$g.md || fallo=1; \
	done; \
	python3 scripts/ll1.py docs/fase2/json-izquierda.ll1 > $(DOCS_TMP)/ll1-json-izquierda.md; \
	diff -u docs/fase2/ll1-json-izquierda.md $(DOCS_TMP)/ll1-json-izquierda.md || fallo=1; \
	if [ $$fallo -ne 0 ]; then \
		echo "docs/ desincronizado con el código: ejecutar 'make docs' y versionar el resultado"; \
		exit 1; \
	fi; \
	echo "docs-check: .dot de la Fase 1, ll1-*.md de la Fase 2, .dot de la IR (Fase 3), del autómata (Fase 4) y de los árboles de derivación (Fase 5) sincronizados"

# Regenera las IR doradas de tests/ir (`schemac ir` de cada esquema de
# tests/schemas/parse-ok con el mismo nombre), las XSD de tests/ir/xsd (de
# tests/schemas/xsd/parse-ok/<nombre>.xsd; tarea 5.4) y los resúmenes del
# autómata de tests/automata (`schemac automaton`, F4.1a). Solo tras un cambio
# de la bajada o del formato revisado a mano: las pruebas comparan contra
# ellas. Escribe en un `.tmp` y lo mueve solo si `schemac` termina bien, así
# una compilación fallida no deja la dorada vacía.
doradas:
	for f in tests/ir/*.json; do \
		cargo run -q -p cli -- ir tests/schemas/parse-ok/$$(basename $$f) > $$f.tmp \
			&& mv $$f.tmp $$f || { rm -f $$f.tmp; exit 1; }; \
	done
	for f in tests/ir/xsd/*.json; do \
		cargo run -q -p cli -- ir tests/schemas/xsd/parse-ok/$$(basename $$f .json).xsd > $$f.tmp \
			&& mv $$f.tmp $$f || { rm -f $$f.tmp; exit 1; }; \
	done
	for f in tests/automata/*.json; do \
		cargo run -q -p cli -- automaton tests/schemas/parse-ok/$$(basename $$f) > $$f.tmp \
			&& mv $$f.tmp $$f || { rm -f $$f.tmp; exit 1; }; \
	done

clean:
	cargo clean
