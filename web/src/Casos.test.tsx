import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Casos, describir } from "./Casos";
import { Tupla } from "./Tupla";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("describir", () => {
  it("pone en palabras lo esperado de cada clase", () => {
    expect(describir({ valid: true, violations: 0 })).toEqual({ clase: "valido", partes: ["válido", "peso 0"] });
    expect(describir({ valid: false, errorKind: "schema", violations: 3, offset: 18, pointer: "/pedido" })).toEqual({
      clase: "invalido",
      partes: ["3 violaciones", "peso 3", "byte 18", "/pedido"],
    });
    expect(describir({ valid: false, errorKind: "lexical", offset: 7 }).partes).toEqual(["error léxico", "peso ∞", "byte 7"]);
    expect(describir({ valid: false, errorKind: "limit", offset: 256 }).clase).toBe("limite");
    expect(describir(null).clase).toBe("otro");
  });
});

describe("Casos", () => {
  it("lista los casos con su descripción y, al ejecutar, lo que dio M", async () => {
    const caso = {
      id: "invalid-x-01", category: "invalid", format: "json",
      description: "Propiedad <b>b</b> no declarada.", expected: { valid: false, errorKind: "schema", violations: 1, offset: 4 },
    };
    vi.stubGlobal("fetch", vi.fn(async (ruta: string) =>
      new Response(
        JSON.stringify(
          ruta.startsWith("/api/tests/run")
            ? { total: 1, passed: 1, failed: 0, skipped: 0, errors: 0,
                results: [{ id: caso.id, category: "invalid", status: "passed", mismatch: [], got: caso.expected, expected: caso.expected }] }
            : { total: 1, cases: [caso] },
        ),
        { status: 200 },
      ),
    ));
    const { container } = render(<Casos />);
    await waitFor(() => expect(screen.getByText("Propiedad <b>b</b> no declarada.")).toBeTruthy());
    expect(container.querySelector(".caso-descripcion b")).toBeNull();
    expect(screen.getByText("1 violación")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Ejecutar 1 casos/ }));
    await waitFor(() => expect(screen.getByText("pasa")).toBeTruthy());
    expect(screen.getAllByText("1 violación")).toHaveLength(2);
  });
});

describe("Tupla", () => {
  it("cada símbolo explica su significado y muestra el valor del esquema compilado", () => {
    const resumen = { automatonVersion: 1, format: "json", states: 16, transitions: 60, frames: 3, classes: 12,
      registerBytes: 8, maxFrameBytes: 40, products: 0, deferred: 0 };
    render(<Tupla resumen={resumen} />);
    fireEvent.click(screen.getByRole("button", { name: /^Q: Estados/ }));
    expect(screen.getByRole("tooltip").textContent).toContain("16 estados");
    fireEvent.click(screen.getByRole("button", { name: /^δ:/ }));
    expect(screen.getByRole("tooltip").textContent).toContain("60 transiciones");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
