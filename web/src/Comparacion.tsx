import { useState } from "react";
import { bytes, duracion, pedirComparacion, type Comparacion as C, type FalloVista } from "./vistas";

// Vista «comparación en vivo» (F7.6): valida el payload con el DPDA (M sobre
// `runtime-rs`) y con la línea base DOM (`serde_json` + `jsonschema`, D48)
// mediante `POST /api/compare`, y muestra veredictos, primer error, tiempos y
// memoria. Los números del corpus y del bombeo están en
// `docs/fase6/metricas.md` (F6.5).

function veredicto(valido: boolean, kind: string | null): string {
  if (valido) return "válido";
  if (kind === "lexical" || kind === "syntax" || kind === "malformed") return "malformado";
  if (kind === "limit") return "límite superado";
  return "inválido";
}

/** Barras de tiempo de validación, proporcionales al mayor. */
function Barras({ filas }: { filas: { nombre: string; ns: number }[] }) {
  const max = Math.max(1, ...filas.map((f) => f.ns));
  return (
    <div className="barras" role="img" aria-label="Tiempo de validación">
      {filas.map((f) => (
        <div key={f.nombre} className="fila-barra">
          <span>{f.nombre}</span>
          <div className="barra-fondo">
            <div className="barra-valor" style={{ width: `${(100 * f.ns) / max}%` }} />
          </div>
          <span>{duracion(f.ns)}</span>
        </div>
      ))}
    </div>
  );
}

interface Props {
  esquema: string;
  payload: string;
  alCambiarPayload: (p: string) => void;
}

export function Comparacion({ esquema, payload, alCambiarPayload }: Props) {
  const [r, setR] = useState<C | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState(false);

  const validar = async () => {
    setCargando(true);
    setError(null);
    const x = await pedirComparacion(esquema, payload);
    setCargando(false);
    if (x.ok) setR(x.datos);
    else {
      setR(null);
      setError(x.fallo);
    }
  };

  const lb = r?.baseline;
  return (
    <div className="vista">
      <label className="payload">
        Payload (JSON o XML):
        <textarea value={payload} onChange={(e) => alCambiarPayload(e.target.value)} rows={5} spellCheck={false} />
      </label>
      <div className="barra">
        <button type="button" className="principal" onClick={validar} disabled={cargando}>
          {cargando ? "Validando…" : "Validar con el DPDA y la línea base"}
        </button>
        {r && (
          <span className={r.agree === false ? "error" : "tenue"}>
            {r.agree === null ? "sin línea base para comparar" : r.agree ? "coinciden" : "¡no coinciden!"} ·{" "}
            {bytes(r.payloadBytes)} de payload
          </span>
        )}
      </div>
      {error && <p className="error">{error.mensaje}</p>}
      {r && (
        <>
          <table className="comparacion">
            <thead>
              <tr>
                <th />
                <th>DPDA (M, streaming)</th>
                <th>Línea base DOM</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <th>Veredicto</th>
                <td>{veredicto(r.dpda.valid, r.dpda.errorKind)}</td>
                <td>{lb?.disponible ? veredicto(lb.valid, lb.kind) : "—"}</td>
              </tr>
              <tr>
                <th>Violaciones</th>
                <td>{r.dpda.violations ?? "∞ (malformado o límite)"}</td>
                <td>{lb?.disponible ? `${lb.violations}${lb.truncated ? "+" : ""}` : "—"}</td>
              </tr>
              <tr>
                <th>Primer error</th>
                <td>
                  {r.dpda.offset == null
                    ? "—"
                    : `${r.dpda.errorKind} en el byte ${r.dpda.offset} (línea ${r.dpda.line}, columna ${r.dpda.column})${r.dpda.rule ? `, regla ${r.dpda.rule}` : ""}`}
                </td>
                <td>
                  {!lb?.disponible
                    ? "—"
                    : lb.malformed
                      ? `byte ${lb.malformed.offset ?? "?"}: ${lb.malformed.message}`
                      : lb.errors[0]
                        ? `${lb.errors[0].pointer || "/"} (${lb.errors[0].keyword}): ${lb.errors[0].message}`
                        : "—"}
                </td>
              </tr>
              <tr>
                <th>Compilar el esquema</th>
                <td>{duracion(r.dpda.compileNs)}</td>
                <td>{lb?.disponible ? duracion(lb.compileNs) : "—"}</td>
              </tr>
              <tr>
                <th>Validar el payload</th>
                <td>{duracion(r.dpda.validateNs)}</td>
                <td>{lb?.disponible ? duracion(lb.validateNs) : "—"}</td>
              </tr>
              <tr>
                <th>Memoria de validación</th>
                <td>{bytes(r.dpda.memoryBytes)} fijos (pila acotada)</td>
                <td>{lb?.disponible ? "árbol DOM: crece con el payload" : "—"}</td>
              </tr>
              <tr>
                <th>Tamaño de M</th>
                <td>
                  {r.dpda.states} estados, {r.dpda.transitions} transiciones
                </td>
                <td>—</td>
              </tr>
            </tbody>
          </table>
          {!lb?.disponible && lb && <p className="tenue">Línea base: {lb.motivo}</p>}
          <Barras
            filas={[
              { nombre: "DPDA", ns: r.dpda.validateNs },
              ...(lb?.disponible ? [{ nombre: "Línea base", ns: lb.validateNs }] : []),
            ]}
          />
          {lb?.disponible && lb.errors.length > 1 && (
            <details>
              <summary>Todas las violaciones de la línea base ({lb.errors.length})</summary>
              <ul>
                {lb.errors.map((e, i) => (
                  <li key={i}>
                    {e.pointer || "/"} ({e.keyword}): {e.message}
                  </li>
                ))}
              </ul>
            </details>
          )}
          <p className="tenue">
            Una sola medición en el servidor: orientativa. Las medidas con repeticiones (MB/s, ns/byte, pico de memoria,
            asignaciones) del corpus y del bombeo están en <code>docs/fase6/metricas.md</code>.
          </p>
        </>
      )}
    </div>
  );
}
