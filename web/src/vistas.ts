// Clientes de las vistas de F7.6 (D70, `docs/fase7/vistas.md`):
// `POST /api/automaton`, `POST /api/derivation` y `POST /api/compare`.
//
// Igual que `api.ts`: todo lo que llega es texto derivado del usuario y se
// muestra solo como texto (React escapa; Cytoscape dibuja en un lienzo). Las
// funciones `interpretar*` son puras y nunca lanzan: así se prueban sin red.

export interface NodoGrafo {
  id: string;
  label: string;
  name?: string;
  group?: number | null;
  accept?: boolean;
  special: boolean;
}

export interface AristaGrafo {
  id: string;
  source: string;
  target: string;
  label: string;
}

export interface Grafo {
  format: string;
  states: number;
  transitions: number;
  start: string;
  groups: string[];
  truncated: boolean;
  nodes: NodoGrafo[];
  edges: AristaGrafo[];
}

/** Nodo del árbol de derivación (F5.5) para `d3.hierarchy`. */
export interface NodoArbol {
  name: string;
  kind: "nt" | "t" | "eps";
  start: number;
  end: number;
  lexeme?: string;
  pointer?: string;
  children?: NodoArbol[];
}

export interface VeredictoDpda {
  valid: boolean;
  violations: number | null;
  errorKind: string | null;
  offset: number | null;
  line: number | null;
  column: number | null;
  rule: string | null;
  compileNs: number;
  validateNs: number;
  memoryBytes: number;
  states: number;
  transitions: number;
}

export interface ErrorLineaBase {
  pointer: string;
  keyword: string;
  message: string;
}

export type VeredictoLineaBase =
  | {
      disponible: true;
      valid: boolean;
      kind: string | null;
      violations: number;
      truncated: boolean;
      errors: ErrorLineaBase[];
      malformed: { offset: number | null; message: string } | null;
      compileNs: number;
      validateNs: number;
    }
  | { disponible: false; motivo: string };

export interface Comparacion {
  format: string;
  payloadBytes: number;
  dpda: VeredictoDpda;
  baseline: VeredictoLineaBase;
  agree: boolean | null;
}

/** Un fallo de cualquier vista, con un mensaje para mostrar. */
export interface FalloVista {
  estado: number | null;
  mensaje: string;
  /** Offset del error en el payload o el esquema, si lo hay. */
  offset?: number | null;
}

export type Respuesta<T> = { ok: true; datos: T } | { ok: false; fallo: FalloVista };

const esObjeto = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const texto = (v: unknown, d = ""): string => (typeof v === "string" ? v : d);
const numero = (v: unknown, d = 0): number => (typeof v === "number" && Number.isFinite(v) ? v : d);
const numeroONulo = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) ? v : null);
const textoONulo = (v: unknown): string | null => (typeof v === "string" ? v : null);

function leer(cuerpo: string): unknown {
  try {
    return cuerpo === "" ? null : JSON.parse(cuerpo);
  } catch {
    return null;
  }
}

/** El fallo de una respuesta que no es 200. */
export function fallo(estado: number, v: unknown): FalloVista {
  if (!esObjeto(v)) return { estado, mensaje: `respuesta inesperada del servidor (${estado})` };
  if (v.error === "schema" && Array.isArray(v.errors)) {
    const primero = v.errors.find(esObjeto);
    const donde = primero ? ` (línea ${numero(primero.line)}, columna ${numero(primero.column)})` : "";
    return {
      estado,
      mensaje: `el esquema no compila${donde}: ${primero ? texto(primero.message) : "ver el editor"}`,
    };
  }
  return { estado, mensaje: texto(v.message, texto(v.error, `error ${estado}`)), offset: numeroONulo(v.offset) };
}

export function interpretarGrafo(estado: number, cuerpo: string): Respuesta<Grafo> {
  const v = leer(cuerpo);
  if (estado !== 200 || !esObjeto(v)) return { ok: false, fallo: fallo(estado, v) };
  const nodos: NodoGrafo[] = (Array.isArray(v.nodes) ? v.nodes : []).filter(esObjeto).map((n) => ({
    id: texto(n.id),
    label: texto(n.label),
    name: texto(n.name),
    group: numeroONulo(n.group),
    accept: n.accept === true,
    special: n.special === true,
  }));
  const ids = new Set(nodos.map((n) => n.id));
  const aristas: AristaGrafo[] = (Array.isArray(v.edges) ? v.edges : [])
    .filter(esObjeto)
    .map((e) => ({ id: texto(e.id), source: texto(e.source), target: texto(e.target), label: texto(e.label) }))
    // Cytoscape lanza con una arista a un nodo que no existe.
    .filter((e) => ids.has(e.source) && ids.has(e.target));
  return {
    ok: true,
    datos: {
      format: texto(v.format),
      states: numero(v.states),
      transitions: numero(v.transitions),
      start: texto(v.start),
      groups: (Array.isArray(v.groups) ? v.groups : []).map((g) => texto(g)),
      truncated: v.truncated === true,
      nodes: nodos,
      edges: aristas,
    },
  };
}

/** Valida la forma del árbol sin recursión (puede tener miles de nodos). */
function esArbol(v: unknown): v is NodoArbol {
  const pila: unknown[] = [v];
  while (pila.length > 0) {
    const n = pila.pop();
    if (!esObjeto(n) || typeof n.name !== "string" || !["nt", "t", "eps"].includes(texto(n.kind))) return false;
    if (n.children !== undefined) {
      if (!Array.isArray(n.children)) return false;
      pila.push(...n.children);
    }
  }
  return true;
}

export function interpretarArbol(estado: number, cuerpo: string): Respuesta<NodoArbol> {
  const v = leer(cuerpo);
  if (estado !== 200) return { ok: false, fallo: fallo(estado, v) };
  if (!esArbol(v)) return { ok: false, fallo: { estado, mensaje: "árbol con una forma inesperada" } };
  return { ok: true, datos: v };
}

export function interpretarComparacion(estado: number, cuerpo: string): Respuesta<Comparacion> {
  const v = leer(cuerpo);
  if (estado !== 200 || !esObjeto(v) || !esObjeto(v.dpda)) return { ok: false, fallo: fallo(estado, v) };
  const d = v.dpda;
  const b = esObjeto(v.baseline) ? v.baseline : {};
  const linea: VeredictoLineaBase =
    typeof b.valid === "boolean"
      ? {
          disponible: true,
          valid: b.valid,
          kind: textoONulo(b.kind),
          violations: numero(b.violations),
          truncated: b.truncated === true,
          errors: (Array.isArray(b.errors) ? b.errors : [])
            .filter(esObjeto)
            .map((e) => ({ pointer: texto(e.pointer), keyword: texto(e.keyword), message: texto(e.message) })),
          malformed: esObjeto(b.malformed)
            ? { offset: numeroONulo(b.malformed.offset), message: texto(b.malformed.message) }
            : null,
          compileNs: numero(b.compileNs),
          validateNs: numero(b.validateNs),
        }
      : { disponible: false, motivo: texto(b.error, "sin línea base") };
  return {
    ok: true,
    datos: {
      format: texto(v.format),
      payloadBytes: numero(v.payloadBytes),
      dpda: {
        valid: d.valid === true,
        violations: numeroONulo(d.violations),
        errorKind: textoONulo(d.errorKind),
        offset: numeroONulo(d.offset),
        line: numeroONulo(d.line),
        column: numeroONulo(d.column),
        rule: textoONulo(d.rule),
        compileNs: numero(d.compileNs),
        validateNs: numero(d.validateNs),
        memoryBytes: numero(d.memoryBytes),
        states: numero(d.states),
        transitions: numero(d.transitions),
      },
      baseline: linea,
      agree: typeof v.agree === "boolean" ? v.agree : null,
    },
  };
}

async function enviar<T>(
  ruta: string,
  cuerpo: string,
  interpretar: (estado: number, cuerpo: string) => Respuesta<T>,
): Promise<Respuesta<T>> {
  try {
    const r = await fetch(ruta, { method: "POST", body: cuerpo });
    return interpretar(r.status, await r.text());
  } catch {
    return { ok: false, fallo: { estado: null, mensaje: "no se pudo contactar con el servidor" } };
  }
}

export const pedirGrafo = (esquema: string) => enviar("/api/automaton", esquema, interpretarGrafo);
export const pedirArbol = (payload: string) => enviar("/api/derivation", payload, interpretarArbol);
export const pedirComparacion = (esquema: string, payload: string) =>
  enviar("/api/compare", JSON.stringify({ schema: esquema, payload }), interpretarComparacion);

/** Nanosegundos legibles. */
export function duracion(ns: number): string {
  if (ns < 1_000) return `${ns} ns`;
  if (ns < 1_000_000) return `${(ns / 1_000).toFixed(1)} µs`;
  if (ns < 1_000_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  return `${(ns / 1_000_000_000).toFixed(2)} s`;
}

/** Bytes legibles. */
export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1 << 20) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1 << 20)).toFixed(1)} MiB`;
}
