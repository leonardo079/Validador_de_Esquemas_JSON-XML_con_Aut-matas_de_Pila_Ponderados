import { useState } from "react";
import { CampoPayload } from "./CampoPayload";
import type { Muestra } from "./ejemplos";
import {
  bytes,
  descargarCodigo,
  validarEnFlujo,
  validarGenerado,
  type FalloVista,
  type Informe,
  type Lenguaje,
  type MensajeFlujo,
  type Validacion,
} from "./vistas";

// Vista «código generado» (F7.5, D81):
// - descargar el validador generado (Rust o C) como ZIP (`POST /api/code`);
// - validar el payload con ese código compilado y ejecutado en el sandbox
//   (`POST /api/validate`), comparado con el intérprete;
// - validar en streaming por bloques sobre `WS /ws/validate`, con progreso.

function resumen(r: Informe | null): string {
  if (!r) return "—";
  if (r.valid) return "válido";
  const donde = r.offset != null ? ` en el byte ${r.offset}` : "";
  const puntero = r.pointer ? ` (${r.pointer || "/"})` : "";
  const v = r.violations == null ? "malformado o límite" : `${r.violations} violación(es)`;
  return `${v}, ${r.errorKind ?? "?"}${donde}${puntero}`;
}

interface Props {
  esquema: string;
  payload: string;
  alCambiarPayload: (p: string) => void;
  /** Muestras del ejemplo elegido en «Esquema» (válido, con violaciones, malformado). */
  muestras?: Muestra[];
}

export function Generado({ esquema, payload, alCambiarPayload, muestras }: Props) {
  const [lang, setLang] = useState<Lenguaje>("c");
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState("");
  const [val, setVal] = useState<Validacion | null>(null);
  const [bloque, setBloque] = useState(64);
  const [flujo, setFlujo] = useState<{ listo?: MensajeFlujo; ultimo?: MensajeFlujo; final?: MensajeFlujo } | null>(null);

  const descargar = async () => {
    setCargando("descargar");
    setError(await descargarCodigo(esquema, lang));
    setCargando("");
  };

  const sandbox = async () => {
    setCargando("sandbox");
    setError(null);
    setVal(null);
    const r = await validarGenerado(esquema, payload, lang);
    setCargando("");
    if (r.ok) setVal(r.datos);
    else setError(r.fallo);
  };

  const enVivo = async () => {
    setCargando("flujo");
    setError(null);
    setFlujo({});
    const final = await validarEnFlujo(esquema, payload, Math.max(1, bloque), (m) =>
      // Solo los `progress` mueven la barra: el `report` final no la vacía.
      setFlujo((f) => (m.type === "ready" ? { ...f, listo: m } : m.type === "progress" ? { ...f, ultimo: m } : f)),
    );
    setFlujo((f) => ({ ...f, final }));
    setCargando("");
  };

  const total = new TextEncoder().encode(payload).length;
  const ultimo = flujo?.ultimo?.type === "progress" ? flujo.ultimo : null;
  return (
    <div className="vista">
      <CampoPayload payload={payload} alCambiar={alCambiarPayload} muestras={muestras} filas={4} />

      <section className="tarjeta">
        <h3>Validador generado</h3>
        <p className="tenue explica">
          M traducido a C11 o a Rust sin dependencias: una tabla de transiciones y una pila de tamaño fijo. Se descarga
          como ZIP o se compila y ejecuta en un sandbox aislado, comparando su veredicto con el del intérprete.
        </p>
      <div className="barra">
        <label>
          Lenguaje:{" "}
          <select value={lang} onChange={(e) => setLang(e.target.value as Lenguaje)}>
            <option value="c">C11</option>
            <option value="rust">Rust</option>
          </select>
        </label>
        <button type="button" onClick={descargar} disabled={cargando !== ""}>
          {cargando === "descargar" ? "Generando…" : "Descargar el código (ZIP)"}
        </button>
        <button type="button" className="principal" onClick={sandbox} disabled={cargando !== ""}>
          {cargando === "sandbox" ? "Compilando en el sandbox…" : "Compilar y validar en el sandbox"}
        </button>
      </div>
      {val && (
        <table className="comparacion">
          <thead>
            <tr>
              <th />
              <th>Código generado ({val.lang === "c" ? "C11" : "Rust"}, sandbox)</th>
              <th>Intérprete (M)</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>Resultado</th>
              <td>{val.status === "ok" ? resumen(val.generated) : `sandbox: ${val.status}`}</td>
              <td>{resumen(val.interpreter)}</td>
            </tr>
            <tr>
              <th>Regla</th>
              <td>{val.generated?.rule ?? "—"}</td>
              <td>{val.interpreter.rule ?? "—"}</td>
            </tr>
            <tr>
              <th>Detalle</th>
              <td>
                código de salida {val.exitCode ?? "—"}; compilar + ejecutar en {val.sandboxMs} ms
              </td>
              <td className={val.agree === false ? "error" : undefined}>
                {val.agree === null ? "—" : val.agree ? "coinciden" : "¡no coinciden!"}
              </td>
            </tr>
          </tbody>
        </table>
      )}

      </section>

      <section className="tarjeta">
        <h3>Validación en streaming (WebSocket)</h3>
        <p className="tenue explica">
          El payload viaja en bloques del tamaño que elijas; M avanza con cada bloque sin guardar los anteriores, con
          la misma memoria fija. Con un bloque de 1 byte el veredicto es el mismo.
        </p>
      <div className="barra">
        <label>
          Bloque:{" "}
          <input type="number" min={1} max={1048576} value={bloque} onChange={(e) => setBloque(Number(e.target.value) || 1)} style={{ width: 90 }} /> bytes
        </label>
        <button type="button" className="principal" onClick={enVivo} disabled={cargando !== ""}>
          {cargando === "flujo" ? "Validando…" : "Validar por bloques"}
        </button>
      </div>
      {flujo && (
        <div>
          {flujo.listo?.type === "ready" && (
            <p className="tenue">
              M: {flujo.listo.states} estados, {flujo.listo.transitions} transiciones; memoria fija{" "}
              {bytes(flujo.listo.memoryBytes)}.
            </p>
          )}
          <div className="barras">
            <div className="fila-barra">
              <span>Enviado</span>
              <div className="barra-fondo">
                <div className="barra-valor" style={{ width: `${total ? (100 * (ultimo?.bytes ?? 0)) / total : 0}%` }} />
              </div>
              <span>
                {bytes(ultimo?.bytes ?? 0)} / {bytes(total)}
              </span>
            </div>
          </div>
          {ultimo && (
            <p className="tenue">
              bloque {ultimo.chunk}
              {ultimo.stopped && " — el validador ya encontró un error de peso ∞ (malformado o límite)"}
            </p>
          )}
          {flujo.final?.type === "report" && (
            <p>
              <strong>Veredicto:</strong> {resumen(flujo.final.report)} — {flujo.final.chunks} bloques, {bytes(flujo.final.bytes)}
            </p>
          )}
          {flujo.final?.type === "error" && <p className="error">{flujo.final.message}</p>}
        </div>
      )}
      </section>
      {error && <p className="error">{error.mensaje}</p>}
    </div>
  );
}
