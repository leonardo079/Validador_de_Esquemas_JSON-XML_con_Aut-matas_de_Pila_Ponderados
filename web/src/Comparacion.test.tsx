import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Comparacion, perfilDePila } from "./Comparacion";
import { cargaDeUsuarios, EJEMPLOS } from "./ejemplos";
import { CampoPayload } from "./CampoPayload";
import { Arbol } from "./Arbol";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function responder(estado: number, cuerpo: unknown) {
  const f = vi.fn(async () => new Response(JSON.stringify(cuerpo), { status: estado }));
  vi.stubGlobal("fetch", f);
  return f;
}

describe("Comparacion", () => {
  it("envía esquema y payload y muestra la tabla; el texto del servidor no se interpreta como HTML", async () => {
    const f = responder(200, {
      format: "json",
      payloadBytes: 9,
      dpda: {
        valid: false, violations: 1, errorKind: "schema", offset: 5, line: 1, column: 6,
        rule: "#/properties/<img src=x onerror=alert(1)>", compileNs: 1000, validateNs: 2000,
        memoryBytes: 73728, states: 17, transitions: 62,
      },
      baseline: {
        valid: false, kind: "schema", violations: 1, truncated: false,
        errors: [{ pointer: "/a", keyword: "type", message: "<b>no</b>" }], malformed: null,
        compileNs: 3000, validateNs: 4000,
      },
      agree: true,
    });
    const { container } = render(<Comparacion esquema='{"type":"object"}' payload='{"a": 1}' alCambiarPayload={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Validar/ }));
    await waitFor(() => expect(screen.getByText("coinciden", { exact: false })).toBeTruthy());
    const [ruta, init] = f.mock.calls[0] as unknown as [string, RequestInit];
    expect(ruta).toBe("/api/compare");
    expect(JSON.parse(init.body as string)).toEqual({ schema: '{"type":"object"}', payload: '{"a": 1}' });
    expect(screen.getAllByText("inválido")).toHaveLength(2);
    expect(screen.getByText(/72.0 KiB fijos/)).toBeTruthy();
    // XSS: ni <img> ni <b> se crean.
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("td b")).toBeNull();
    expect(screen.getByText(/onerror=alert/)).toBeTruthy();
  });

  it("muestra el error del servidor", async () => {
    responder(400, { error: "request", message: "faltan `schema` o `payload` (cadenas)" });
    render(<Comparacion esquema="" payload="" alCambiarPayload={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Validar/ }));
    await waitFor(() => expect(screen.getByText(/faltan/)).toBeTruthy());
  });
});

describe("Arbol", () => {
  it("dibuja los nodos del árbol en SVG y muestra el detalle al elegir uno", async () => {
    responder(200, {
      name: "value", kind: "nt", start: 0, end: 4, pointer: "",
      children: [{ name: "true", kind: "t", start: 0, end: 4, lexeme: "true" }],
    });
    const { container } = render(<Arbol payload="true" alCambiarPayload={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /árbol de derivación/i }));
    await waitFor(() => expect(container.querySelectorAll("svg g.nodo")).toHaveLength(2));
    fireEvent.click(container.querySelectorAll("svg g.nodo")[1]);
    expect(screen.getByText(/lexema "true"/)).toBeTruthy();
  });

  it("un payload malformado muestra el byte del error", async () => {
    responder(422, { error: "derivation", kind: "syntax", offset: 3, message: "error sintáctico en el byte 3: se esperaba value" });
    render(<Arbol payload="[1,]" alCambiarPayload={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /árbol de derivación/i }));
    await waitFor(() => expect(screen.getByText(/se esperaba value/)).toBeTruthy());
  });
});

describe("perfilDePila", () => {
  it("cuenta el anidamiento de JSON sin contar llaves dentro de cadenas", () => {
    const p = perfilDePila('{"a": [1, {"b": "}]{["}], "c": 2}');
    expect(p.maximo).toBe(3);
    expect(p.total).toBe(33);
  });
  it("cuenta las etiquetas de XML; `/>`, `<?` y `<!--` no apilan", () => {
    const p = perfilDePila('<?xml version="1.0"?><a><!-- x --><b><c/></b></a>');
    expect(p.maximo).toBe(2);
  });
  it("la carga de usuarios es válida salvo un `id: 0`", () => {
    const carga = cargaDeUsuarios(8);
    const filas = JSON.parse(carga) as { id: number }[];
    expect(filas).toHaveLength(8);
    expect(filas.filter((f) => f.id === 0)).toHaveLength(1);
    expect(perfilDePila(carga).maximo).toBe(2);
  });
});

describe("Comparacion con medición", () => {
  it("muestra el banco de pruebas con las asignaciones medidas", async () => {
    const medido = {
      valido: true, violaciones: 0, segundos: 0.001, mbPorSegundo: 30, nsPorByte: 33,
      asignaciones: 1, picoBytes: 75776, repeticiones: 15,
    };
    responder(200, {
      format: "json", payloadBytes: 9,
      dpda: { valid: true, violations: 0, errorKind: null, offset: null, line: null, column: null, rule: null,
        compileNs: 1, validateNs: 1, memoryBytes: 75776, states: 3, transitions: 4 },
      baseline: { valid: true, kind: null, violations: 0, truncated: false, errors: [], malformed: null, compileNs: 1, validateNs: 1 },
      agree: true,
      measurement: { bytes: 9, profundidad: 1, dpda: medido,
        lineaBase: { ...medido, asignaciones: 40015, picoBytes: 14_000_000 }, motivoLineaBase: null },
    });
    render(<Comparacion esquema="{}" payload='{"a": 1}' alCambiarPayload={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Validar/ }));
    await waitFor(() => expect(screen.getByLabelText("Banco de pruebas en vivo")).toBeTruthy());
    expect(screen.getByText("185×")).toBeTruthy();
    expect(screen.getByText(/13.4 MiB de pico: árbol DOM/)).toBeTruthy();
  });
});

describe("ejemplos y muestras de payload", () => {
  it("cada ejemplo, salvo el de errores, trae válido, con violaciones y malformado", () => {
    for (const e of EJEMPLOS.filter((x) => x.nombre !== "Con errores")) {
      expect(e.muestras.map((m) => m.tipo), e.nombre).toEqual(["valido", "violaciones", "malformado"]);
    }
    const claves = EJEMPLOS.map((e) => e.grupo + e.nombre);
    expect(new Set(claves).size).toBe(claves.length);
  });
  it("pulsar una muestra pone su payload", () => {
    const e = EJEMPLOS[0];
    const alCambiar = vi.fn();
    render(<CampoPayload payload={e.muestras[0].payload} alCambiar={alCambiar} muestras={e.muestras} />);
    expect(screen.getByRole("button", { name: /^válido/ }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: /malformado/ }));
    expect(alCambiar).toHaveBeenCalledWith(e.muestras[2].payload);
  });
});
