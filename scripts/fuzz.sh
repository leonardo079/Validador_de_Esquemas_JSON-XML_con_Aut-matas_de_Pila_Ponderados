#!/bin/sh
# Fuzzing con cargo-fuzz (F6.4, docs/fase6/fuzzing.md): los cinco objetivos
# de fuzz/ durante <segundos> cada uno (60 por defecto) y, después, lo que
# encontraron (el corpus de validar_json y validar_xml) por la prueba
# diferencial intérprete / Rust / C (F5.3), que compila el C con ASan/UBSan.
# Necesita nightly (`rustup toolchain install nightly`) y cargo-fuzz.
# Uso: scripts/fuzz.sh [segundos]
set -eu
SEG=${1:-60}
cd "$(dirname "$0")/.."
HOST=$(rustc -vV | sed -n 's/^host: //p')
python3 scripts/fuzz_semillas.py fuzz/corpus
# Un objetivo que falla no detiene los demás ni la reproducción en C; el
# script sale con 1 si algo falló (el fallo queda en fuzz/artifacts/).
fallos=""
for t in validar_json validar_xml esquema_json esquema_xsd esquema_dtd; do
	echo "== $t ($SEG s)"
	cargo +nightly fuzz run --fuzz-dir fuzz -O --target "$HOST" "$t" "fuzz/corpus/$t" -- \
		-max_total_time="$SEG" -max_len=4096 -rss_limit_mb=4096 -timeout=20 -print_final_stats=1 ||
		fallos="$fallos $t"
done
echo "== reproducción en el C (prueba diferencial)"
SV_EXTRA_JSON=fuzz/corpus/validar_json SV_EXTRA_XML=fuzz/corpus/validar_xml \
	cargo test -p codegen --test diferencial -- --nocapture || fallos="$fallos diferencial"
if [ -n "$fallos" ]; then
	echo "fuzz: fallaron:$fallos"
	exit 1
fi
echo "fuzz: sin fallos"
