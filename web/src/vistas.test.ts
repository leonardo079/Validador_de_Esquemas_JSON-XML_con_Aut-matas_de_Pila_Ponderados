import { describe, expect, it } from "vitest";
import { bytes, duracion, interpretarArbol, interpretarComparacion, interpretarGrafo } from "./vistas";
import { elementos } from "./Automata";
import { disenar } from "./Arbol";

const GRAFO = {
  format: "json",
  states: 2,
  transitions: 3,
  start: "q0",
  groups: ["#"],
  truncated: false,
  nodes: [
    { id: "q0", label: "q0", name: "valor", group: 0, accept: false, special: false },
    { id: "q1", label: "q1", name: "fin", group: 0, accept: true, special: false },
    { id: "desapila", label: "↩ marco", special: true },
  ],
  edges: [
    { id: "e0", source: "q0", target: "q1", label: "{ / apila" },
    { id: "e1", source: "q1", target: "desapila", label: "} / desapila" },
    { id: "e2", source: "q1", target: "q9", label: "huérfana" },
  ],
};

describe("interpretarGrafo", () => {
  it("lee nodos y aristas y quita las que apuntan a nodos que no llegaron", () => {
    const r = interpretarGrafo(200, JSON.stringify(GRAFO));
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.datos.nodes).toHaveLength(3);
    expect(r.datos.edges.map((e) => e.id)).toEqual(["e0", "e1"]);
    expect(r.datos.nodes[1].accept).toBe(true);
  });
  it("un 422 de esquema da su primer error con la posición", () => {
    const r = interpretarGrafo(
      422,
      JSON.stringify({ error: "schema", errors: [{ code: "meta-schema", line: 1, column: 10, message: "type no válido" }] }),
    );
    expect(r).toEqual({
      ok: false,
      fallo: { estado: 422, mensaje: "el esquema no compila (línea 1, columna 10): type no válido" },
    });
  });
  it("un cuerpo que no es JSON no lanza", () => {
    expect(interpretarGrafo(502, "<html>").ok).toBe(false);
    expect(interpretarGrafo(200, "[]").ok).toBe(false);
  });
});

describe("elementos de Cytoscape", () => {
  it("marca inicio, aceptación y especiales; etiqueta con el nombre", () => {
    const r = interpretarGrafo(200, JSON.stringify(GRAFO));
    if (!r.ok) throw new Error("grafo");
    const el = elementos(r.datos);
    expect(el).toHaveLength(5);
    const q0 = el.find((e) => e.data.id === "q0")!;
    expect(q0.classes).toContain("inicio");
    expect(q0.data.label).toBe("q0\nvalor");
    expect(el.find((e) => e.data.id === "q1")!.classes).toContain("acepta");
    expect(el.find((e) => e.data.id === "desapila")!.classes).toContain("especial");
  });
});

const ARBOL = {
  name: "value",
  kind: "nt",
  start: 0,
  end: 8,
  pointer: "",
  children: [
    {
      name: "array",
      kind: "nt",
      start: 0,
      end: 8,
      children: [
        { name: "[", kind: "t", start: 0, end: 1, lexeme: "[" },
        {
          name: "elements",
          kind: "nt",
          start: 1,
          end: 7,
          children: [
            { name: "value", kind: "nt", start: 1, end: 5, pointer: "/0", children: [{ name: "true", kind: "t", start: 1, end: 5, lexeme: "true" }] },
            { name: "elements_tail", kind: "nt", start: 5, end: 5, children: [{ name: "ε", kind: "eps", start: 5, end: 5 }] },
          ],
        },
        { name: "]", kind: "t", start: 7, end: 8, lexeme: "]" },
      ],
    },
  ],
};

describe("interpretarArbol y disenar", () => {
  it("acepta un árbol bien formado y lo dispone por niveles", () => {
    const r = interpretarArbol(200, JSON.stringify(ARBOL));
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const d = disenar(r.datos);
    expect(d.nodos).toHaveLength(9);
    expect(d.enlaces).toHaveLength(8);
    // La raíz arriba y cada hijo un nivel más abajo que su padre.
    expect(d.nodos[0].y).toBe(0);
    for (const l of d.enlaces) expect(l.a.y).toBeGreaterThan(l.de.y);
    // Los hermanos, de izquierda a derecha en el orden de la derivación.
    const hijos = d.nodos.filter((n) => n.parent?.data.name === "array");
    expect(hijos.map((n) => n.data.name)).toEqual(["[", "elements", "]"]);
    expect(hijos[0].x).toBeLessThan(hijos[1].x);
    expect(hijos[1].x).toBeLessThan(hijos[2].x);
    expect(d.ancho).toBeGreaterThan(0);
  });
  it("rechaza formas inesperadas y da el offset de un 422", () => {
    expect(interpretarArbol(200, JSON.stringify({ name: "x", kind: "raro" })).ok).toBe(false);
    expect(interpretarArbol(200, JSON.stringify({ name: "x", kind: "nt", children: [{ kind: "t" }] })).ok).toBe(false);
    const r = interpretarArbol(422, JSON.stringify({ error: "derivation", kind: "syntax", offset: 3, message: "error sintáctico en el byte 3" }));
    expect(r).toEqual({ ok: false, fallo: { estado: 422, mensaje: "error sintáctico en el byte 3", offset: 3 } });
  });
});

describe("interpretarComparacion", () => {
  const cuerpo = {
    format: "json",
    payloadBytes: 17,
    dpda: {
      valid: false,
      violations: 2,
      errorKind: "schema",
      offset: 6,
      line: 1,
      column: 7,
      rule: "#/properties/a",
      compileNs: 50000,
      validateNs: 2500,
      memoryBytes: 75776,
      states: 23,
      transitions: 75,
    },
    baseline: {
      valid: false,
      kind: "schema",
      violations: 2,
      truncated: false,
      errors: [{ pointer: "/a", keyword: "maximum", message: "30 is greater than 10" }],
      malformed: null,
      compileNs: 90000,
      validateNs: 8000,
    },
    agree: true,
  };
  it("lee los dos veredictos", () => {
    const r = interpretarComparacion(200, JSON.stringify(cuerpo));
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.datos.dpda.violations).toBe(2);
    expect(r.datos.baseline.disponible).toBe(true);
    expect(r.datos.agree).toBe(true);
  });
  it("sin línea base (XSD) y con violaciones ∞ (null)", () => {
    const r = interpretarComparacion(
      200,
      JSON.stringify({ ...cuerpo, dpda: { ...cuerpo.dpda, violations: null }, baseline: { error: "sin libxml2" }, agree: null }),
    );
    if (!r.ok) throw new Error("comparación");
    expect(r.datos.dpda.violations).toBeNull();
    expect(r.datos.baseline).toEqual({ disponible: false, motivo: "sin libxml2" });
    expect(r.datos.agree).toBeNull();
  });
  it("un 400 da su mensaje", () => {
    const r = interpretarComparacion(400, JSON.stringify({ error: "request", message: "faltan `schema` o `payload`" }));
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.fallo.mensaje).toBe("faltan `schema` o `payload`");
  });
});

describe("unidades", () => {
  it("duracion y bytes", () => {
    expect(duracion(500)).toBe("500 ns");
    expect(duracion(2500)).toBe("2.5 µs");
    expect(duracion(3_500_000)).toBe("3.50 ms");
    expect(bytes(512)).toBe("512 B");
    expect(bytes(75776)).toBe("74.0 KiB");
    expect(bytes(3 << 20)).toBe("3.0 MiB");
  });
});

import { interpretarCasos, interpretarEjecucion } from "./vistas";

describe("casos de Atlas (F7.2)", () => {
  it("interpreta la lista y la ejecución", () => {
    const l = interpretarCasos(
      200,
      JSON.stringify({ total: 1, cases: [{ id: "valid-x-01", category: "valid", format: "json", description: "d", expected: { valid: true } }] }),
    );
    expect(l.ok && l.datos[0].id).toBe("valid-x-01");
    const e = interpretarEjecucion(
      200,
      JSON.stringify({
        total: 2, passed: 1, failed: 1, skipped: 0, errors: 0,
        results: [
          { id: "a", category: "valid", status: "passed", mismatch: [], got: { valid: true }, expected: { valid: true } },
          { id: "b", category: "invalid", status: "raro", mismatch: ["valid"] },
        ],
      }),
    );
    expect(e.ok).toBe(true);
    if (!e.ok) return;
    expect(e.datos.passed).toBe(1);
    expect(e.datos.results[1].status).toBe("error");
    expect(e.datos.results[1].got).toBeNull();
  });
  it("sin base de datos da el mensaje del 503", () => {
    const r = interpretarCasos(503, JSON.stringify({ error: "no-database", message: "el servidor no tiene base de datos" }));
    expect(r).toEqual({ ok: false, fallo: { estado: 503, mensaje: "el servidor no tiene base de datos", offset: null } });
  });
});
