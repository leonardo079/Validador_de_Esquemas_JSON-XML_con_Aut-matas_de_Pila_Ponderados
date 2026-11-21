import { describe, expect, it } from "vitest";
import { interpretarMensaje, interpretarValidacion, validarEnFlujo, type MensajeFlujo } from "./vistas";

describe("POST /api/validate (F7.5)", () => {
  it("lee el resultado del sandbox y del intérprete", () => {
    const r = interpretarValidacion(
      200,
      JSON.stringify({
        lang: "c", status: "ok", exitCode: 1, sandboxMs: 900, agree: true,
        generated: { valid: false, violations: 1, errorKind: "schema", offset: 6, pointer: "/a", rule: "#/properties/a" },
        interpreter: { valid: false, violations: 1, errorKind: "schema", offset: 6, pointer: "/a", rule: "#/properties/a" },
      }),
    );
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.datos.exitCode).toBe(1);
    expect(r.datos.generated?.pointer).toBe("/a");
    expect(r.datos.agree).toBe(true);
  });
  it("sin sandbox da el mensaje del 503", () => {
    const r = interpretarValidacion(503, JSON.stringify({ error: "no-sandbox", message: "el servidor no tiene sandbox" }));
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.fallo.mensaje).toBe("el servidor no tiene sandbox");
  });
});

describe("WS /ws/validate (F7.5)", () => {
  it("interpreta los cuatro tipos de mensaje", () => {
    expect(interpretarMensaje('{"type":"ready","format":"json","states":3,"transitions":5,"memoryBytes":100}').type).toBe("ready");
    expect(interpretarMensaje('{"type":"progress","chunk":2,"bytes":10,"stopped":true}')).toEqual({ type: "progress", chunk: 2, bytes: 10, stopped: true });
    const r = interpretarMensaje('{"type":"report","bytes":10,"chunks":2,"report":{"valid":true,"violations":0}}');
    expect(r.type === "report" && r.report.valid).toBe(true);
    expect(interpretarMensaje('{"type":"error","message":"x"}')).toEqual({ type: "error", message: "x" });
    expect(interpretarMensaje("no json").type).toBe("error");
  });

  it("manda el esquema, un bloque tras cada progreso y `fin`", async () => {
    const enviados: (string | number)[] = [];
    class Falso {
      binaryType = "";
      onopen: (() => void) | null = null;
      onmessage: ((ev: { data: string }) => void) | null = null;
      onerror: (() => void) | null = null;
      onclose: (() => void) | null = null;
      bloques = 0;
      constructor(public url: string) {
        setTimeout(() => this.onopen?.(), 0);
      }
      send(d: string | Uint8Array) {
        enviados.push(typeof d === "string" ? d : d.length);
        const responder = (m: object) => setTimeout(() => this.onmessage?.({ data: JSON.stringify(m) }), 0);
        if (typeof d === "string" && d === "fin") responder({ type: "report", bytes: 10, chunks: this.bloques, report: { valid: true, violations: 0 } });
        else if (typeof d === "string") responder({ type: "ready", format: "json", states: 1, transitions: 1, memoryBytes: 8 });
        else responder({ type: "progress", chunk: ++this.bloques, bytes: 0, stopped: false });
      }
      close() {}
    }
    const vistos: MensajeFlujo[] = [];
    const final = await validarEnFlujo('{"type":"array"}', "[1,2,3,4]", 4, (m) => vistos.push(m), (u) => new Falso(u) as unknown as WebSocket);
    expect(enviados).toEqual(['{"type":"array"}', 4, 4, 1, "fin"]);
    expect(vistos.map((m) => m.type)).toEqual(["ready", "progress", "progress", "progress", "report"]);
    expect(final.type).toBe("report");
  });
});
