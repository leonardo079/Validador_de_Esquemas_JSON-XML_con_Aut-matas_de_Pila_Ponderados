import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Resultado } from "./Resultado";
import { interpretar } from "./api";

afterEach(cleanup);

const XSS = '<img src=x onerror="alert(1)">';

describe("Resultado", () => {
  it("un nombre de propiedad hostil en la IR se muestra como texto, no como HTML", () => {
    const cuerpo = JSON.stringify({
      apiVersion: 1, id: "1", format: "json", summary: XSS, irBytes: 10, irOmitted: false,
      automaton: { status: "pending", message: XSS },
      ir: { nodes: [{ properties: { [XSS]: 1 } }] },
    });
    const { container } = render(<Resultado resultado={interpretar(200, cuerpo)} cargando={false} />);
    fireEvent.click(screen.getByRole("button", { name: /Ver IR/ }));
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain(XSS);
    expect(container.querySelector("pre.ir")?.textContent).toContain("onerror");
  });

  it("los mensajes de error hostiles del 422 se muestran como texto", () => {
    const cuerpo = JSON.stringify({
      error: "schema", format: "json", total: 1, truncated: false,
      errors: [{ code: "syntax", line: 2, column: 3, message: XSS }],
    });
    const elegidos: number[] = [];
    const { container } = render(
      <Resultado resultado={interpretar(422, cuerpo)} cargando={false} alElegirError={(e) => elegidos.push(e.line)} />,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText(/1 error/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "línea 2, columna 3" }));
    expect(elegidos).toEqual([2]);
  });

  it("muestra el resumen del autómata", () => {
    const cuerpo = JSON.stringify({
      apiVersion: 1, id: "1", format: "json", summary: "ok", irBytes: null, irOmitted: true,
      automaton: { status: "ok", summary: { states: 24, transitions: 77, frames: 3, classes: 16, registerBytes: 8, maxFrameBytes: 16, products: 0, deferred: 0 } },
      ir: null,
    });
    render(<Resultado resultado={interpretar(200, cuerpo)} cargando={false} />);
    expect(screen.getByText("Estados").nextSibling?.textContent).toBe("24");
    expect(screen.getByText(/supera el tope/)).toBeTruthy();
  });

  it("muestra el fallo con su estado", () => {
    render(<Resultado resultado={interpretar(413, JSON.stringify({ error: "too-large", message: "demasiado grande" }))} cargando={false} />);
    expect(screen.getByRole("alert").textContent).toContain("demasiado grande");
    expect(screen.getByRole("alert").textContent).toContain("413");
  });
});
