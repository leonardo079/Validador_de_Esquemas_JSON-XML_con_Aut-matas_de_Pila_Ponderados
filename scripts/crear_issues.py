"""Crea un issue de GitHub por cada tarea `Tx.y` del plan de desarrollo (T0.5).

Por defecto solo muestra lo que crearía. Para crear los issues de verdad:

    set GITHUB_TOKEN=<token con permiso de issues>
    python scripts/crear_issues.py --plan ../PLAN_DESARROLLO.md --crear

Las subtareas (p. ej. T4.5.1) quedan como lista de verificación dentro del
issue de su tarea padre. Cada issue lleva las etiquetas `tarea` y `fase-N`.
Los issues cuyo título ya existe en el repositorio se omiten.
"""

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request

REPO_POR_DEFECTO = "leonardo079/Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados"
API = "https://api.github.com"

RE_FASE = re.compile(r"^## Fase (\d+) — (.+)$")
RE_TAREA = re.compile(r"^- \[[ x]\] \*\*(T\d+\.\d+)\*\* (.+)$")
RE_SUBTAREA = re.compile(r"^\s+- \[[ x]\] (T\d+\.\d+\.\d+) (.+)$")


def leer_tareas(ruta):
    tareas = []
    fase = None
    actual = None
    with open(ruta, encoding="utf-8") as f:
        for linea in f:
            linea = linea.rstrip("\n")
            if m := RE_FASE.match(linea):
                fase = (int(m.group(1)), m.group(2).strip())
                actual = None
            elif linea.startswith("## "):
                fase, actual = None, None
            elif fase and (m := RE_TAREA.match(linea)):
                actual = {"id": m.group(1), "titulo": m.group(2), "fase": fase, "cuerpo": []}
                tareas.append(actual)
            elif actual and (m := RE_SUBTAREA.match(linea)):
                actual["cuerpo"].append(f"- [ ] **{m.group(1)}** {m.group(2)}")
            elif actual and linea.startswith("  ") and linea.strip():
                actual["cuerpo"].append(linea.strip())
            elif actual and not linea.strip():
                actual = None
    return tareas


def titulo_corto(tarea):
    texto = re.sub(r"\*\*|`", "", tarea["titulo"])
    texto = texto.split(": ")[0] if len(texto) > 90 else texto
    return f'{tarea["id"]} — {texto[:110].rstrip(" .:")}'


def cuerpo(tarea):
    num, nombre = tarea["fase"]
    lineas = [f"**Fase {num} — {nombre}**", "", tarea["titulo"], ""]
    lineas += tarea["cuerpo"]
    return "\n".join(lineas)


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


def asegurar_etiqueta(repo, token, nombre):
    try:
        pedir("POST", f"/repos/{repo}/labels", token, {"name": nombre, "color": "0e8a16"})
    except urllib.error.HTTPError as e:
        if e.code != 422:  # 422 = ya existe
            raise


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--plan", default="../PLAN_DESARROLLO.md")
    p.add_argument("--repo", default=REPO_POR_DEFECTO)
    p.add_argument("--crear", action="store_true", help="crear los issues (sin esto, solo muestra)")
    args = p.parse_args()

    tareas = leer_tareas(args.plan)
    if not args.crear:
        for t in tareas:
            print(f'[fase-{t["fase"][0]}] {titulo_corto(t)}')
        print(f"\n{len(tareas)} tareas. Use --crear para crearlas en {args.repo}.")
        return

    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        sys.exit("Falta la variable de entorno GITHUB_TOKEN.")

    existentes = titulos_existentes(args.repo, token)
    asegurar_etiqueta(args.repo, token, "tarea")
    for num in sorted({t["fase"][0] for t in tareas}):
        asegurar_etiqueta(args.repo, token, f"fase-{num}")

    for t in tareas:
        titulo = titulo_corto(t)
        if titulo in existentes:
            print(f"omitido (ya existe): {titulo}")
            continue
        issue = pedir("POST", f"/repos/{args.repo}/issues", token, {
            "title": titulo,
            "body": cuerpo(t),
            "labels": ["tarea", f'fase-{t["fase"][0]}'],
        })
        print(f'#{issue["number"]} {titulo}')


if __name__ == "__main__":
    main()
