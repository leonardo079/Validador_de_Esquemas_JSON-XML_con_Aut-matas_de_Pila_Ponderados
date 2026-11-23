import { describe, expect, it } from "vitest";
import tabla from "../../tests/schemas/formato.json";
import { formatoProbable, type Formato } from "./api";

// D86 (E2): la copia en TypeScript de `dtd::formato` contra la misma tabla
// que prueba Rust (`crates/dtd/tests/formato.rs`).
describe("formatoProbable con la tabla común", () => {
  it("tiene al menos la tabla de la enmienda E1", () => expect(tabla.casos.length).toBeGreaterThanOrEqual(9));
  it.each(tabla.casos.map((c) => [c.texto, c.formato as Formato] as const))("%j → %s", (texto, formato) =>
    expect(formatoProbable(texto)).toBe(formato),
  );
});
