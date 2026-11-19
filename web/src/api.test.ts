import { describe, expect, it } from "vitest";
import { compilar, formatoProbable, interpretar } from "./api";

const OK = JSON.stringify({
  apiVersion: 1,
  id: "ab12",
  format: "json",
  summary: "esquema válido (subconjunto de JSON Schema 2020-12); 3 subesquemas",
  irBytes: 20,
  irOmitted: false,
  automaton: {
    status: "ok",
    summary: { automatonVersion: 1, format: "json", states: 24, transitions: 77, frames: 3, classes: 16, registerBytes: 8, maxFrameBytes: 16, products: 0, deferred: 0 },
  },
  ir: { irVersion: 2, format: "json" },
});

describe("interpretar", () => {
  it("200 con autómata e IR", () => {
    const r = interpretar(200, OK);
    expect(r.tipo).toBe("ok");
    if (r.tipo !== "ok") return;
    expect(r.id).toBe("ab12");
    expect(r.automaton.status).toBe("ok");
    expect(r.ir).toBe('{"irVersion":2,"format":"json"}');
  });

  it("la IR se muestra cruda: los u64 enormes no se redondean (§10)", () => {
    const ir = '{\n  "irVersion": 2,\n  "max": 18446744073709551615\n}\n';
    const cuerpo =
      '{"apiVersion":1,"id":"x","format":"json","summary":"s ,\\"ir\\": no","irBytes":40,"irOmitted":false,' +
      '"automaton":{"status":"pending","message":"m"},"ir":' + ir + "}";
    const r = interpretar(200, cuerpo);
    expect(r.tipo).toBe("ok");
    if (r.tipo !== "ok") return;
    expect(r.summary).toBe('s ,"ir": no');
    expect(r.ir).toContain("18446744073709551615");
  });

  it("200 con autómata pendiente e IR omitida", () => {
    const cuerpo = JSON.stringify({
      apiVersion: 1, id: "x", format: "xml", summary: "s", irBytes: null, irOmitted: true,
      automaton: { status: "pending", message: "estrategia pendiente: modelo de contenido (F4.1c) en #" },
      ir: null,
    });
    const r = interpretar(200, cuerpo);
    expect(r.tipo).toBe("ok");
    if (r.tipo !== "ok") return;
    expect(r.format).toBe("xml");
    expect(r.ir).toBeNull();
    expect(r.irOmitted).toBe(true);
    expect(r.automaton).toEqual({ status: "pending", message: "estrategia pendiente: modelo de contenido (F4.1c) en #" });
  });

  it("422 con errores truncados", () => {
    const cuerpo = JSON.stringify({
      error: "schema", format: "json", total: 150, truncated: true,
      errors: [{ code: "meta-schema", line: 1, column: 11, message: "valor inválido" }],
    });
    const r = interpretar(422, cuerpo);
    expect(r).toEqual({
      tipo: "esquema", format: "json", total: 150, truncated: true,
      errors: [{ code: "meta-schema", line: 1, column: 11, message: "valor inválido" }],
    });
  });

  it("503 con Retry-After", () => {
    const r = interpretar(503, JSON.stringify({ error: "busy", message: "ocupado" }), "5");
    expect(r).toEqual({ tipo: "fallo", estado: 503, codigo: "busy", mensaje: "ocupado", reintentarEn: 5 });
  });

  it("400 vacío de hyper y cuerpo no JSON no lanzan", () => {
    expect(interpretar(400, "")).toMatchObject({ tipo: "fallo", estado: 400, codigo: "http" });
    expect(interpretar(502, "<html>Bad Gateway</html>")).toMatchObject({ tipo: "fallo", estado: 502 });
    expect(interpretar(200, "no es json")).toMatchObject({ tipo: "fallo" });
  });

  it("valores con tipos raros no rompen la interpretación", () => {
    const r = interpretar(422, JSON.stringify({ error: "schema", errors: [{ line: "x" }, 5, null], total: "?" }));
    expect(r.tipo).toBe("esquema");
    if (r.tipo !== "esquema") return;
    expect(r.errors).toEqual([{ code: "?", line: 1, column: 1, message: "" }]);
    expect(r.total).toBe(1);
  });
});

describe("compilar", () => {
  it("envía el esquema crudo a /api/schemas", async () => {
    let peticion: { url: string; init?: RequestInit } | null = null;
    const falso = (async (url: string, init?: RequestInit) => {
      peticion = { url, init };
      return new Response(OK, { status: 200 });
    }) as unknown as typeof fetch;
    const r = await compilar('{"type":"object"}', falso);
    expect(r.tipo).toBe("ok");
    expect(peticion).not.toBeNull();
    expect(peticion!.url).toBe("/api/schemas");
    expect(peticion!.init?.method).toBe("POST");
    expect(peticion!.init?.body).toBe('{"type":"object"}');
  });

  it("error de red", async () => {
    const falso = (async () => {
      throw new TypeError("Failed to fetch");
    }) as unknown as typeof fetch;
    expect(await compilar("{}", falso)).toMatchObject({ tipo: "fallo", estado: null, codigo: "red" });
  });
});

describe("formatoProbable (D22)", () => {
  it.each([
    ["{}", "json"],
    ["  \n<xs:schema/>", "xml"],
    ["﻿<x/>", "xml"],
    ["[1]", "json"],
    ["", "json"],
  ])("%j → %s", (t, f) => expect(formatoProbable(t)).toBe(f));
});
