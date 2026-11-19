// Cliente de `POST /api/schemas` (contrato de F7.1, `docs/fase7/servidor.md` §2).
//
// Todo lo que llega del servidor es texto del usuario o derivado de él (claves,
// nombres, literales, mensajes): se trata como **no confiable** y la interfaz lo
// muestra solo como texto (React escapa por defecto; nunca `innerHTML` ni
// `dangerouslySetInnerHTML`, `docs/fase7/pendientes.md` §5).

/** Resumen del autómata (F4-U7, `write_summary_json`). */
export interface ResumenAutomata {
  automatonVersion: number;
  format: string;
  states: number;
  transitions: number;
  frames: number;
  classes: number;
  registerBytes: number;
  maxFrameBytes: number;
  products: number;
  deferred: number;
}

export type Automata =
  | { status: "ok"; summary: ResumenAutomata }
  | { status: "pending" | "limit"; message: string };

/** Un error del esquema (422). `line` y `column` empiezan en 1. */
export interface ErrorEsquema {
  code: string;
  line: number;
  column: number;
  message: string;
}

/** Resultado de compilar, ya interpretado. */
export type Resultado =
  | {
      tipo: "ok";
      id: string;
      format: "json" | "xml";
      summary: string;
      irBytes: number | null;
      irOmitted: boolean;
      automaton: Automata;
      /** La IR v2 reformateada para leerla, o `null` si el servidor la omitió. */
      ir: string | null;
    }
  | {
      tipo: "esquema";
      format: string;
      errors: ErrorEsquema[];
      total: number;
      truncated: boolean;
    }
  | {
      /** Cualquier otro error: 413, 408, 500, 503, 504, 404, 405, 400 o la red. */
      tipo: "fallo";
      estado: number | null;
      codigo: string;
      mensaje: string;
      reintentarEn: number | null;
    };

function esObjeto(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function texto(v: unknown, defecto = ""): string {
  return typeof v === "string" ? v : defecto;
}

function numero(v: unknown, defecto = 0): number {
  return typeof v === "number" && Number.isFinite(v) ? v : defecto;
}

/** Mensaje en español para los estados sin cuerpo JSON (400/431 de hyper, proxy). */
function mensajePorEstado(estado: number): string {
  switch (estado) {
    case 400:
      return "petición mal formada";
    case 413:
      return "el esquema es demasiado grande";
    case 431:
      return "cabeceras de la petición demasiado grandes";
    case 502:
    case 503:
      return "el servidor no está disponible; vuelve a intentarlo";
    case 504:
      return "la compilación tardó demasiado";
    default:
      return `respuesta inesperada del servidor (${estado})`;
  }
}

/**
 * El texto de la IR tal como lo escribió `ir::write_json`, sin pasar por
 * `JSON.parse`: así los contadores de hasta `u64::MAX` no se redondean
 * (`docs/fase3/pendientes.md` §10). Funciona porque la IR va al final del 200
 * (B12 de F7.1) y la cabecera es JSON compacto: dentro de una cadena JSON las
 * comillas van escapadas, así que `,"ir":` solo puede ser la clave de la IR.
 */
function irCruda(cuerpo: string): string | null {
  const i = cuerpo.indexOf(',"ir":');
  if (i < 0) return null;
  const resto = cuerpo.slice(i + ',"ir":'.length).trimEnd();
  if (!resto.endsWith("}")) return null;
  const ir = resto.slice(0, -1).trim();
  return ir === "null" || ir === "" ? null : ir;
}

/**
 * Interpreta una respuesta del servidor. Pura: no toca la red, así se prueba
 * con cuerpos fabricados. Nunca lanza: un cuerpo inesperado da `fallo`.
 */
export function interpretar(estado: number, cuerpo: string, retryAfter: string | null = null): Resultado {
  let v: unknown = null;
  try {
    v = cuerpo === "" ? null : JSON.parse(cuerpo);
  } catch {
    v = null;
  }
  const reintentarEn = retryAfter !== null && /^\d+$/.test(retryAfter) ? Number(retryAfter) : null;

  if (estado === 200 && esObjeto(v)) {
    const a = esObjeto(v.automaton) ? v.automaton : {};
    let automaton: Automata;
    if (a.status === "ok" && esObjeto(a.summary)) {
      automaton = { status: "ok", summary: a.summary as unknown as ResumenAutomata };
    } else {
      automaton = {
        status: a.status === "limit" ? "limit" : "pending",
        message: texto(a.message, "sin resumen del autómata"),
      };
    }
    return {
      tipo: "ok",
      id: texto(v.id),
      format: v.format === "xml" ? "xml" : "json",
      summary: texto(v.summary),
      irBytes: typeof v.irBytes === "number" ? v.irBytes : null,
      irOmitted: v.irOmitted === true,
      automaton,
      ir: v.ir === null || v.ir === undefined ? null : (irCruda(cuerpo) ?? JSON.stringify(v.ir, null, 2)),
    };
  }

  if (estado === 422 && esObjeto(v) && v.error === "schema" && Array.isArray(v.errors)) {
    const errors: ErrorEsquema[] = v.errors.filter(esObjeto).map((e) => ({
      code: texto(e.code, "?"),
      line: Math.max(1, numero(e.line, 1)),
      column: Math.max(1, numero(e.column, 1)),
      message: texto(e.message),
    }));
    return {
      tipo: "esquema",
      format: texto(v.format, "json"),
      errors,
      total: numero(v.total, errors.length),
      truncated: v.truncated === true,
    };
  }

  if (esObjeto(v) && typeof v.error === "string") {
    return {
      tipo: "fallo",
      estado,
      codigo: v.error,
      mensaje: texto(v.message, mensajePorEstado(estado)),
      reintentarEn,
    };
  }
  return { tipo: "fallo", estado, codigo: "http", mensaje: mensajePorEstado(estado), reintentarEn };
}

/** Compila `esquema` contra el servidor. `fetch` se inyecta en las pruebas. */
export async function compilar(esquema: string, fetchImpl: typeof fetch = fetch): Promise<Resultado> {
  try {
    const r = await fetchImpl("/api/schemas", {
      method: "POST",
      // El servidor detecta el formato por el contenido (D22); el tipo no importa.
      headers: { "Content-Type": "application/octet-stream" },
      body: esquema,
    });
    return interpretar(r.status, await r.text(), r.headers.get("Retry-After"));
  } catch {
    return {
      tipo: "fallo",
      estado: null,
      codigo: "red",
      mensaje: "no se pudo contactar con el servidor (¿está corriendo `cargo run -p server`?)",
      reintentarEn: null,
    };
  }
}

/** Mismo criterio que el servidor (D22): primer byte significativo `<` → XSD. */
export function formatoProbable(esquema: string): "json" | "xml" {
  const s = esquema.replace(/^﻿/, "").trimStart();
  return s.startsWith("<") ? "xml" : "json";
}
