#!/usr/bin/env python3
"""Semillas del fuzzing (F6.4, docs/fase6/fuzzing.md): los payloads del corpus
(tests/cases) y los esquemas de tests/schemas, un archivo por entrada, en
<dir>/<objetivo>/. Solo añade: no borra lo que el fuzzer ya encontró.

Uso: python3 scripts/fuzz_semillas.py fuzz/corpus
"""
import base64
import hashlib
import json
import pathlib
import sys

RAIZ = pathlib.Path(__file__).resolve().parent.parent


def guardar(destino: pathlib.Path, datos: bytes) -> None:
    destino.mkdir(parents=True, exist_ok=True)
    (destino / ("semilla-" + hashlib.sha1(datos).hexdigest())).write_bytes(datos)


def main() -> None:
    salida = pathlib.Path(sys.argv[1])
    for caso in sorted((RAIZ / "tests" / "cases").glob("*/*.json")):
        c = json.loads(caso.read_text(encoding="utf-8"))
        if "payload" in c:
            datos = c["payload"].encode("utf-8")
        elif "payloadBase64" in c:
            datos = base64.b64decode(c["payloadBase64"])
        elif "payloadFile" in c:
            datos = (caso.parent / c["payloadFile"]).read_bytes()
        else:
            continue
        if len(datos) > 4096:
            continue
        guardar(salida / ("validar_xml" if c["format"] == "xml" else "validar_json"), datos)
    for e in sorted((RAIZ / "tests" / "schemas").rglob("*.json")):
        if e.name.startswith("errores"):
            continue
        guardar(salida / "esquema_json", e.read_bytes())
    for e in sorted((RAIZ / "tests" / "schemas" / "xsd").rglob("*.xsd")):
        guardar(salida / "esquema_xsd", e.read_bytes())
    # DTD.5 (D86): las DTD válidas y las rechazadas, como semillas de esquema_dtd.
    for e in sorted((RAIZ / "tests" / "schemas" / "dtd").rglob("*.dtd")):
        guardar(salida / "esquema_dtd", e.read_bytes())


if __name__ == "__main__":
    main()
