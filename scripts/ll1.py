"""Calcula FIRST, FOLLOW y la tabla LL(1) de una gramática y detecta conflictos.

Formato de la gramática (una producción por línea, `#` inicia comentario):

    value -> object | array | STRING
    members -> pair members_tail | ε

El primer no terminal es el símbolo inicial. Son no terminales los símbolos
que aparecen a la izquierda de `->`; los demás son terminales. `ε` es la
cadena vacía y `$` el fin de la entrada.

Uso:

    python3 scripts/ll1.py docs/fase2/json.ll1 > docs/fase2/ll1-json.md

Escribe el informe en Markdown y termina con código 1 si hay conflictos.
"""

import sys

EPS = "ε"
END = "$"


def leer(ruta):
    reglas = []  # (no terminal, [alternativas]) en orden de aparición
    with open(ruta, encoding="utf-8") as f:
        for n, linea in enumerate(f, 1):
            linea = linea.split("#", 1)[0].strip()
            if not linea:
                continue
            if "->" not in linea:
                sys.exit(f"{ruta}:{n}: falta '->'")
            izq, der = (p.strip() for p in linea.split("->", 1))
            alts = [a.split() for a in der.split("|")]
            alts = [[] if a == [EPS] else a for a in alts]
            reglas.append((izq, alts))
    return reglas


def first_de(seq, first):
    """FIRST de una secuencia de símbolos."""
    res = set()
    for s in seq:
        f = first.get(s, {s})
        res |= f - {EPS}
        if EPS not in f:
            return res
    return res | {EPS}


def calcular(reglas):
    nts = [nt for nt, _ in reglas]
    prods = {nt: [] for nt in nts}
    for nt, alts in reglas:
        prods[nt].extend(alts)
    first = {nt: set() for nt in nts}
    cambio = True
    while cambio:
        cambio = False
        for nt in nts:
            for alt in prods[nt]:
                f = first_de(alt, first)
                if not f <= first[nt]:
                    first[nt] |= f
                    cambio = True
    follow = {nt: set() for nt in nts}
    follow[nts[0]].add(END)
    cambio = True
    while cambio:
        cambio = False
        for nt in nts:
            for alt in prods[nt]:
                for i, s in enumerate(alt):
                    if s not in prods:
                        continue
                    resto = first_de(alt[i + 1:], first)
                    nuevo = (resto - {EPS}) | (follow[nt] if EPS in resto else set())
                    if not nuevo <= follow[s]:
                        follow[s] |= nuevo
                        cambio = True
    tabla = {}  # (nt, terminal) -> [alternativas]
    for nt in nts:
        for alt in prods[nt]:
            f = first_de(alt, first)
            destinos = (f - {EPS}) | (follow[nt] if EPS in f else set())
            for t in destinos:
                tabla.setdefault((nt, t), []).append(alt)
    terminales = sorted({s for alts in prods.values() for a in alts for s in a if s not in prods})
    return nts, prods, first, follow, tabla, terminales + [END]


def fmt_alt(alt):
    return " ".join(alt) if alt else EPS


def fmt_set(s):
    orden = sorted(s - {EPS, END}) + [x for x in (END, EPS) if x in s]
    return ", ".join(f"`{x}`" for x in orden)


def celda(texto):
    return texto.replace("|", "\\|")


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.stdout.reconfigure(encoding="utf-8")
    ruta = sys.argv[1]
    nts, prods, first, follow, tabla, terms = calcular(leer(ruta))
    conflictos = {k: v for k, v in tabla.items() if len(v) > 1}

    print(f"<!-- Generado por `python3 scripts/ll1.py {ruta}`. No editar a mano. -->\n")
    print("## Gramática\n")
    print("```")
    ancho = max(len(nt) for nt in nts)
    for nt in nts:
        print(f"{nt:<{ancho}} → " + " | ".join(fmt_alt(a) for a in prods[nt]))
    print("```\n")
    print(f"Símbolo inicial: `{nts[0]}`. Terminales: {fmt_set(set(terms) - {END})}.\n")

    print("## Conjuntos FIRST y FOLLOW\n")
    print("| No terminal | FIRST | FOLLOW |")
    print("|---|---|---|")
    for nt in nts:
        print(f"| `{nt}` | {celda(fmt_set(first[nt]))} | {celda(fmt_set(follow[nt]))} |")

    print("\n## Tabla LL(1)\n")
    print("Celda (A, a): producción que el parser aplica con A en la cima de la pila y `a` como")
    print("siguiente token. Una celda vacía es un error sintáctico.\n")
    usados = [t for t in terms if any((nt, t) in tabla for nt in nts)]
    print("| | " + " | ".join(f"`{celda(t)}`" for t in usados) + " |")
    print("|---|" + "---|" * len(usados))
    for nt in nts:
        fila = []
        for t in usados:
            alts = tabla.get((nt, t), [])
            fila.append(celda(" / ".join(fmt_alt(a) for a in alts)) + (" ⚠" if len(alts) > 1 else ""))
        print(f"| `{nt}` | " + " | ".join(fila) + " |")

    print("\n## Conflictos\n")
    if conflictos:
        for (nt, t), alts in sorted(conflictos.items()):
            print(f"- (`{nt}`, `{t}`): " + " / ".join(f"`{fmt_alt(a)}`" for a in alts))
        print("\n**La gramática no es LL(1).**")
        sys.exit(1)
    print("Ninguno: cada celda tiene a lo sumo una producción, así que **la gramática es LL(1)**.")


if __name__ == "__main__":
    main()
