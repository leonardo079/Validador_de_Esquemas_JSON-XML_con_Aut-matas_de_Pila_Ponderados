import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Comparacion } from "./Comparacion";
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
    fireEvent.click(screen.getByRole("button", { name: /Árbol de derivación/ }));
    await waitFor(() => expect(container.querySelectorAll("svg g.nodo")).toHaveLength(2));
    fireEvent.click(container.querySelectorAll("svg g.nodo")[1]);
    expect(screen.getByText(/lexema "true"/)).toBeTruthy();
  });

  it("un payload malformado muestra el byte del error", async () => {
    responder(422, { error: "derivation", kind: "syntax", offset: 3, message: "error sintáctico en el byte 3: se esperaba value" });
    render(<Arbol payload="[1,]" alCambiarPayload={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: /Árbol de derivación/ }));
    await waitFor(() => expect(screen.getByText(/se esperaba value/)).toBeTruthy());
  });
});
