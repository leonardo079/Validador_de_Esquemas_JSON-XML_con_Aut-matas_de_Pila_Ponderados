import { useState } from "react";
import { ejecutarCasos, pedirCasos, type CasoListado, type Ejecucion, type FalloVista } from "./vistas";

// Vista «casos de prueba» (F7.2, D72): los casos del corpus guardados en
// MongoDB Atlas. Se listan con `GET /api/tests` y se ejecutan en el servidor
// con el DPDA (`POST /api/tests/run`), comparando con su `expected`.

const CATEGORIAS = ["", "valid", "invalid", "malformed", "non-dpda", "extreme", "pumping"];
const ETIQUETA: Record<string, string> = { passed: "✔ pasa", failed: "✘ falla", skipped: "– se salta", error: "! error" };

const esperado = (e: Record<string, unknown> | null) =>
  e ? Object.entries(e).map(([k, v]) => `${k}=${String(v)}`).join(" ") : "";

export function Casos() {
  const [categoria, setCategoria] = useState("");
  const [casos, setCasos] = useState<CasoListado[] | null>(null);
  const [ejecucion, setEjecucion] = useState<Ejecucion | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState<"" | "listar" | "ejecutar">("");

  const listar = async () => {
    setCargando("listar");
    setError(null);
    setEjecucion(null);
    const r = await pedirCasos(categoria);
    setCargando("");
    if (r.ok) setCasos(r.datos);
    else setError(r.fallo);
  };

  const ejecutar = async () => {
    setCargando("ejecutar");
    setError(null);
    const r = await ejecutarCasos(categoria);
    setCargando("");
    if (r.ok) setEjecucion(r.datos);
    else setError(r.fallo);
  };

  return (
    <div className="vista">
      <div className="barra">
        <label>
          Categoría:{" "}
          <select value={categoria} onChange={(e) => setCategoria(e.target.value)}>
            {CATEGORIAS.map((c) => (
              <option key={c} value={c}>
                {c || "todas"}
              </option>
            ))}
          </select>
        </label>
        <button type="button" onClick={listar} disabled={cargando !== ""}>
          {cargando === "listar" ? "Consultando…" : "Listar casos de Atlas"}
        </button>
        <button type="button" className="principal" onClick={ejecutar} disabled={cargando !== ""}>
          {cargando === "ejecutar" ? "Ejecutando…" : "Ejecutar con el DPDA"}
        </button>
      </div>
      {error && <p className="error">{error.mensaje}</p>}
      {ejecucion && (
        <p className={ejecucion.failed + ejecucion.errors > 0 ? "error" : "tenue"}>
          {ejecucion.total} casos: {ejecucion.passed} pasan, {ejecucion.failed} fallan, {ejecucion.skipped} se saltan,{" "}
          {ejecucion.errors} con error.
        </p>
      )}
      {ejecucion ? (
        <table className="comparacion">
          <thead>
            <tr>
              <th>Caso</th>
              <th>Resultado</th>
              <th>Esperado</th>
              <th>Obtenido</th>
            </tr>
          </thead>
          <tbody>
            {ejecucion.results.map((r) => (
              <tr key={r.id} className={r.status === "failed" || r.status === "error" ? "fila-error" : undefined}>
                <td>
                  <code>{r.id}</code>
                </td>
                <td>
                  {ETIQUETA[r.status]}
                  {r.mismatch.length > 0 && ` (${r.mismatch.join(", ")})`}
                  {r.reason && <div className="tenue">{r.reason}</div>}
                </td>
                <td>{esperado(r.expected)}</td>
                <td>{esperado(r.got)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        casos && (
          <table className="comparacion">
            <thead>
              <tr>
                <th>Caso ({casos.length})</th>
                <th>Formato</th>
                <th>Descripción</th>
                <th>Esperado</th>
              </tr>
            </thead>
            <tbody>
              {casos.map((c) => (
                <tr key={c.id}>
                  <td>
                    <code>{c.id}</code>
                  </td>
                  <td>{c.format.toUpperCase()}</td>
                  <td>{c.description}</td>
                  <td>{esperado(c.expected)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )
      )}
    </div>
  );
}
