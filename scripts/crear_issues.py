"""Crea un issue de GitHub por cada tarea del cronograma del plan de desarrollo.

Lee las casillas `- [ ]` / `- [x]` de la sección "Cronograma semana a semana".
Cada issue lleva las etiquetas `tarea` y `semana-N`; la persona sugerida va
en el cuerpo (el reparto es tentativo, así que no se asigna). Las tareas ya
marcadas `[x]` se crean cerradas. Los issues cuyo título ya existe se omiten.

Por defecto solo muestra lo que crearía. Para crearlos de verdad:

    export GITHUB_TOKEN=<token con permiso de issues>
    python3 scripts/crear_issues.py --crear
"""

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request

REPO_POR_DEFECTO = "leonardo079/Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados"
PLAN_POR_DEFECTO = "../../Plan de desarrollo — Validador JSON XML con DPDA ponderado.md"
API = "https://api.github.com"

RE_SEMANA = re.compile(r"^### Semana (\d+) \((.+?)\): (.+)$")
RE_GRUPO = re.compile(r"^\*\*(Persona [AB]|Ambos)\b.*\*\*$")
RE_TAREA = re.compile(r"^- \[([ x])\] (.+)$")


def leer_tareas(ruta):
    tareas = []
    semana = grupo = None
    with open(ruta, encoding="utf-8") as f:
        for linea in f:
            linea = linea.rstrip("\n")
            if m := RE_SEMANA.match(linea):
                semana, grupo = (int(m.group(1)), m.group(2), m.group(3)), None
            elif linea.startswith("## "):
                semana = grupo = None
            elif semana and (m := RE_GRUPO.match(linea)):
                grupo = m.group(1)
            elif semana and grupo and (m := RE_TAREA.match(linea)):
                tareas.append({
                    "semana": semana,
                    "grupo": grupo,
                    "hecha": m.group(1) == "x",
                    "texto": m.group(2),
                })
    return tareas


def titulo(tarea):
    texto = re.sub(r"\*\*|`|\\", "", tarea["texto"]).split(" — ")[0]
    if len(texto) > 100:
        texto = texto[:100].rsplit(" ", 1)[0] + "…"
    return f'S{tarea["semana"][0]} · {texto}'


def cuerpo(tarea):
    num, fechas, tema = tarea["semana"]
    sugerido = "ambos" if tarea["grupo"] == "Ambos" else tarea["grupo"]
    return "\n".join([
        f"**Semana {num} ({fechas}):** {tema}",
        f"**Sugerido:** {sugerido} (tentativo, según disponibilidad)",
        "",
        tarea["texto"],
    ])


def pedir(metodo, ruta, token, datos=None):
    req = urllib.request.Request(
        API + ruta,
        method=metodo,
        data=json.dumps(datos).encode() if datos is not None else None,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read() or "null")


def titulos_existentes(repo, token):
    titulos, pagina = set(), 1
    while True:
        lote = pedir("GET", f"/repos/{repo}/issues?state=all&per_page=100&page={pagina}", token)
        if not lote:
            return titulos
        titulos |= {i["title"] for i in lote}
        pagina += 1


def asegurar_etiqueta(repo, token, nombre, color):
    try:
        pedir("POST", f"/repos/{repo}/labels", token, {"name": nombre, "color": color})
    except urllib.error.HTTPError as e:
        if e.code != 422:  # 422 = ya existe
            raise


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--plan", default=PLAN_POR_DEFECTO)
    p.add_argument("--repo", default=REPO_POR_DEFECTO)
    p.add_argument("--crear", action="store_true", help="crear los issues (sin esto, solo muestra)")
    args = p.parse_args()

    tareas = leer_tareas(args.plan)
    if not tareas:
        sys.exit(f"No se encontraron tareas en {args.plan}.")
    if not args.crear:
        for t in tareas:
            estado = "[x]" if t["hecha"] else "[ ]"
            print(f'{estado} {titulo(t)}   ({t["grupo"]})')
        hechas = sum(t["hecha"] for t in tareas)
        print(f"\n{len(tareas)} tareas ({hechas} hechas). Use --crear para crearlas en {args.repo}.")
        return

    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        sys.exit("Falta la variable de entorno GITHUB_TOKEN.")

    existentes = titulos_existentes(args.repo, token)
    asegurar_etiqueta(args.repo, token, "tarea", "0e8a16")
    for num in sorted({t["semana"][0] for t in tareas}):
        asegurar_etiqueta(args.repo, token, f"semana-{num}", "1d76db")

    for t in tareas:
        nombre = titulo(t)
        if nombre in existentes:
            print(f"omitido (ya existe): {nombre}")
            continue
        issue = pedir("POST", f"/repos/{args.repo}/issues", token, {
            "title": nombre,
            "body": cuerpo(t),
            "labels": ["tarea", f'semana-{t["semana"][0]}'],
        })
        if t["hecha"]:
            pedir("PATCH", f'/repos/{args.repo}/issues/{issue["number"]}', token,
                  {"state": "closed", "state_reason": "completed"})
        print(f'#{issue["number"]} {"(cerrado) " if t["hecha"] else ""}{nombre}')


if __name__ == "__main__":
    main()
