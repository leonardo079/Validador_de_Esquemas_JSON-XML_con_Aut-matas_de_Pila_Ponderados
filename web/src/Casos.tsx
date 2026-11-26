import { useEffect, useMemo, useState } from "react";
import { ejecutarCasos, pedirCasos, type CasoListado, type Ejecucion, type FalloVista, type ResultadoCaso } from "./vistas";

// Vista «casos de prueba» (F7.2, D72): los casos del corpus guardados en
// MongoDB Atlas. Se listan al abrir la pestaña (`GET /api/tests`), con su
// descripción, y se ejecutan en el servidor con el DPDA
// (`POST /api/tests/run`), comparando con su `expected`. Cada caso muestra en
// palabras lo que se espera y lo que dio M.

/** Qué comprueba cada categoría del corpus. */
export const CATEGORIAS: Record<string, { nombre: string; texto: string }> = {
  valid: {
    nombre: "Válidos",
    texto: "Payloads que cumplen su esquema. M debe aceptarlos con peso 0.",
  },
  invalid: {
    nombre: "Inválidos",
    texto:
      "Payloads bien formados que violan el esquema. El peso debe ser el número exacto de violaciones, y el primer error, el byte exacto.",
  },
  malformed: {
    nombre: "Malformados",
    texto:
      "No son JSON ni XML bien formado (error léxico o sintáctico). M debe pararse con peso ∞ en el byte del error.",
  },
  "non-dpda": {
    nombre: "Fuera de un DPDA",
    texto:
      "Esquemas que un autómata de pila determinista no expresa directamente (anyOf ambiguo, not, allOf): el compilador los resuelve con un producto o una determinización acotados.",
  },
  extreme: {
    nombre: "Extremos",
    texto: "Entradas en el borde de los límites: 256 niveles de anidamiento y lexemas de 1 024 bytes. Se espera peso ∞ por límite.",
  },
  pumping: {
    nombre: "Bombeo",
    texto: "Familias del lema de bombeo: muestran por qué un autómata finito no basta y hace falta la pila.",
  },
};

type Clase = "valido" | "invalido" | "malformado" | "limite" | "otro";

/** Lo esperado u obtenido, en palabras. */
export function describir(e: Record<string, unknown> | null): { clase: Clase; partes: string[] } {
  if (!e) return { clase: "otro", partes: ["—"] };
  const donde: string[] = [];
  if (typeof e.offset === "number") donde.push(`byte ${e.offset}`);
  if (typeof e.pointer === "string" && e.pointer) donde.push(e.pointer);
  if (e.valid === true) return { clase: "valido", partes: ["válido", "peso 0"] };
  switch (e.errorKind) {
    case "schema": {
      const k = typeof e.violations === "number" ? e.violations : "?";
      return { clase: "invalido", partes: [`${k} ${k === 1 ? "violación" : "violaciones"}`, `peso ${k}`, ...donde] };
    }
    case "lexical":
      return { clase: "malformado", partes: ["error léxico", "peso ∞", ...donde] };
    case "syntax":
      return { clase: "malformado", partes: ["error sintáctico", "peso ∞", ...donde] };
    case "limit":
      return { clase: "limite", partes: ["límite superado", "peso ∞", ...donde] };
    default:
      return { clase: "otro", partes: Object.entries(e).map(([k, v]) => `${k}=${String(v)}`) };
  }
}

function Veredicto({ e, etiqueta }: { e: Record<string, unknown> | null; etiqueta: string }) {
  const d = describir(e);
  return (
    <div className="veredicto-caso">
      <span className="rotulo">{etiqueta}</span>
      <span className={`pastilla ${d.clase}`}>{d.partes[0]}</span>
      {d.partes.slice(1).map((p) => (
        <span key={p} className="dato-caso">
          {p}
        </span>
      ))}
    </div>
  );
}

/** Las descripciones de Atlas marcan el código entre comillas invertidas. */
function conCodigo(s: string) {
  // La barra de una elección «(a|b)» se separa: en Manrope se confunde con una «l».
  return s.split("`").map((p, i) => (i % 2 ? <code key={i}>{p}</code> : p.replace(/\s*\|\s*/g, " | ")));
}

const ORDEN = Object.keys(CATEGORIAS);

const ESTADO: Record<ResultadoCaso["status"], string> = {
  passed: "pasa",
  failed: "falla",
  skipped: "se salta",
  error: "error",
};

export function Casos() {
  const [categoria, setCategoria] = useState("");
  const [busqueda, setBusqueda] = useState("");
  const [casos, setCasos] = useState<CasoListado[] | null>(null);
  const [ejecucion, setEjecucion] = useState<Ejecucion | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState<"" | "listar" | "ejecutar">("");

  const listar = async () => {
    setCargando("listar");
    setError(null);
    const r = await pedirCasos("");
    setCargando("");
    if (r.ok) setCasos(r.datos);
    else setError(r.fallo);
  };

  // La lista (con las descripciones) se carga al abrir la pestaña.
  useEffect(() => {
    void listar();
  }, []);

  const ejecutar = async () => {
    setCargando("ejecutar");
    setError(null);
    const r = await ejecutarCasos(categoria);
    setCargando("");
    if (r.ok) setEjecucion(r.datos);
    else setError(r.fallo);
  };

  const resultados = useMemo(() => new Map((ejecucion?.results ?? []).map((r) => [r.id, r])), [ejecucion]);
  const cuenta = useMemo(() => {
    const c = new Map<string, number>();
    for (const x of casos ?? []) c.set(x.category, (c.get(x.category) ?? 0) + 1);
    return c;
  }, [casos]);
  const q = busqueda.trim().toLowerCase();
  const visibles = (casos ?? []).filter(
    (c) =>
      (!categoria || c.category === categoria) &&
      (!q || c.id.toLowerCase().includes(q) || c.description.toLowerCase().includes(q)),
  );
  const info = CATEGORIAS[categoria];

  return (
    <div className="vista">
      <div className="filtros-casos">
        <div className="segmentos" role="group" aria-label="Categoría">
          {["", ...[...cuenta.keys()].sort((a, b) => (ORDEN.indexOf(a) + 1 || 99) - (ORDEN.indexOf(b) + 1 || 99))].map((c) => (
            <button key={c} type="button" aria-pressed={categoria === c} onClick={() => setCategoria(c)}>
              {c ? (CATEGORIAS[c]?.nombre ?? c) : "Todos"}
              <span>{c ? cuenta.get(c) : (casos?.length ?? 0)}</span>
            </button>
          ))}
        </div>
        <input
          type="search"
          className="buscar"
          placeholder="Buscar por nombre o descripción…"
          value={busqueda}
          onChange={(e) => setBusqueda(e.target.value)}
          aria-label="Buscar casos"
        />
        <button type="button" className="principal" onClick={ejecutar} disabled={cargando !== ""}>
          {cargando === "ejecutar"
            ? "Ejecutando…"
            : `Ejecutar ${categoria ? (cuenta.get(categoria) ?? "") : (casos?.length ?? "")} casos con el DPDA`}
        </button>
      </div>

      {info && <p className="info-categoria">{info.texto}</p>}
      {error && <p className="error">{error.mensaje}</p>}
      {cargando === "listar" && <p className="tenue">Consultando MongoDB Atlas…</p>}

      {ejecucion && (
        <div className="resumen-casos" role="status">
          <div className="dato">
            <span className="cifra">{ejecucion.total}</span>
            <span className="rotulo">ejecutados</span>
          </div>
          <div className="dato ok">
            <span className="cifra">{ejecucion.passed}</span>
            <span className="rotulo">pasan</span>
          </div>
          <div className={ejecucion.failed > 0 ? "dato mal" : "dato"}>
            <span className="cifra">{ejecucion.failed}</span>
            <span className="rotulo">fallan</span>
          </div>
          <div className="dato">
            <span className="cifra">{ejecucion.skipped}</span>
            <span className="rotulo">se saltan</span>
          </div>
          <div className={ejecucion.errors > 0 ? "dato mal" : "dato"}>
            <span className="cifra">{ejecucion.errors}</span>
            <span className="rotulo">con error</span>
          </div>
          <div className="proporcion" aria-hidden="true">
            <span style={{ width: `${ejecucion.total ? (100 * ejecucion.passed) / ejecucion.total : 0}%` }} />
          </div>
        </div>
      )}

      {casos && (
        <ol className="lista-casos" aria-label={`Casos (${visibles.length})`}>
          {visibles.map((c) => {
            const r = resultados.get(c.id);
            return (
              <li key={c.id} className={`caso ${r ? r.status : ""}`}>
                <div className="caso-cabeza">
                  <span className={`estado-caso ${r?.status ?? "pendiente"}`}>{r ? ESTADO[r.status] : "sin ejecutar"}</span>
                  <code>{c.id}</code>
                  <span className="etiqueta">{c.format.toUpperCase()}</span>
                  <span className="etiqueta">{CATEGORIAS[c.category]?.nombre ?? c.category}</span>
                </div>
                <p className="caso-descripcion">{c.description ? conCodigo(c.description) : "Sin descripción."}</p>
                <div className="caso-veredictos">
                  <Veredicto e={c.expected} etiqueta="Se espera" />
                  {r && <Veredicto e={r.got} etiqueta="M dio" />}
                </div>
                {r && r.mismatch.length > 0 && <p className="error">No coincide en: {r.mismatch.join(", ")}</p>}
                {r?.reason && <p className="tenue">{r.reason}</p>}
              </li>
            );
          })}
          {visibles.length === 0 && <li className="tenue">Ningún caso coincide con la búsqueda.</li>}
        </ol>
      )}
    </div>
  );
}
