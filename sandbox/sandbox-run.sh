#!/bin/sh
# Punto de entrada del sandbox (F7.3, D68).
# Uso: sandbox-run <c|rust> <segundos para compilar> <segundos para ejecutar>
#
# Lee de la entrada estándar un tar con los fuentes y el archivo `payload`,
# compila con una orden fija según el lenguaje y ejecuta el binario con el
# payload como entrada estándar y un tope de tiempo. Códigos de salida propios:
#   100 = error de compilación (el diagnóstico va a stderr)
#   101 = el tar no se pudo leer
#   102 = lenguaje o argumentos desconocidos
#   103 = la compilación pasó su tope de tiempo
#   124 = la ejecución pasó el tope de tiempo (`timeout`)
# Cualquier otro es el del binario (137 = lo mató el núcleo: memoria o señal).
set -u

lenguaje="${1:-}"
compilar="${2:-30}"
segundos="${3:-5}"
case "$compilar$segundos" in
    ''|*[!0-9]*) exit 102 ;;
esac

mkdir -p /tmp/src /tmp/out || exit 101
tar -x -f - -C /tmp/src --no-same-owner --no-same-permissions || exit 101
[ -f /tmp/src/payload ] || exit 101
cd /tmp/src || exit 101

case "$lenguaje" in
    c)
        set -- /tmp/src/*.c
        [ -f "$1" ] || { echo "no hay archivos .c" >&2; exit 100; }
        timeout -k 1 "$compilar" cc -std=c11 -O2 -Wall -Wextra -o /tmp/out/validador "$@" \
            2>/tmp/out/compilacion.txt
        ;;
    rust)
        [ -f /tmp/src/main.rs ] || { echo "falta main.rs" >&2; exit 100; }
        timeout -k 1 "$compilar" rustc --edition 2021 -O -o /tmp/out/validador /tmp/src/main.rs \
            2>/tmp/out/compilacion.txt
        ;;
    *)
        exit 102
        ;;
esac
estado=$?
if [ "$estado" -ne 0 ]; then
    head -c 65536 /tmp/out/compilacion.txt >&2
    [ "$estado" -eq 124 ] && exit 103
    exit 100
fi

exec timeout -k 1 "$segundos" /tmp/out/validador </tmp/src/payload
