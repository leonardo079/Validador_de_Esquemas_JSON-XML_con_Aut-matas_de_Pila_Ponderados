import { useState } from "react";
import { NOMBRE_FORMATO, type ErrorEsquema, type Formato, type Resultado as R } from "./api";

// Panel de resultados. Todo se pinta como texto: React escapa los valores, y no
// se usa `dangerouslySetInnerHTML` en ninguna parte (la IR y los mensajes traen
// texto del usuario sin escapar, `docs/fase7/pendientes.md` §5).

interface Props {
  resultado: R | null;
  cargando: boolean;
  /** Mueve el cursor del editor al error elegido. */
  alElegirError?: (e: ErrorEsquema) => void;
  /**
   * Formato del esquema compilado. La API dice `"xml"` para XSD y para DTD
   * (D86, E2), así que la web lo distingue con su propia detección.
   */
  formato?: Formato;
}

/** Etiqueta del formato: la detectada, o la de la API si no se pasó. */
function etiqueta(format: string, formato?: Formato): string {
  return NOMBRE_FORMATO[formato ?? (format === "xml" ? "xsd" : "json")];
}

const ETIQUETAS: Record<string, string> = {
  states: "Estados",
  transitions: "Transiciones",
  frames: "Marcos",
  classes: "Clases de eventos",
  registerBytes: "Bytes de registros",
  maxFrameBytes: "Bytes por marco (máx.)",
  products: "Productos",
  deferred: "Verificaciones diferidas",
};

export function Resultado({ resultado, cargando, alElegirError, formato }: Props) {
  const [verIr, setVerIr] = useState(false);

  if (cargando) return <p className="estado">Compilando…</p>;
  if (resultado === null) {
    return <p className="estado vacio">Escribe o carga un esquema y pulsa «Compilar». El servidor lo analiza, lo traduce a la IR y construye su autómata de pila M.</p>;
  }

  if (resultado.tipo === "fallo") {
    return (
      <div className="caja fallo" role="alert">
        <h2>No se pudo compilar</h2>
        <p>
          {resultado.mensaje}
          {resultado.estado !== null && <span className="tenue"> ({resultado.estado} · {resultado.codigo})</span>}
        </p>
        {resultado.reintentarEn !== null && (
          <p className="tenue">Vuelve a intentarlo en {resultado.reintentarEn} s.</p>
        )}
      </div>
    );
  }

  if (resultado.tipo === "esquema") {
    return (
      <div className="caja error">
        <h2>
          Esquema no válido ({etiqueta(resultado.format, formato)}):{" "}
          {resultado.total} {resultado.total === 1 ? "error" : "errores"}
        </h2>
        {resultado.truncated && (
          <p className="tenue">Se muestran los {resultado.errors.length} primeros.</p>
        )}
        <ol className="errores">
          {resultado.errors.map((e, i) => (
            <li key={i}>
              <button type="button" className="enlace" onClick={() => alElegirError?.(e)}>
                línea {e.line}, columna {e.column}
              </button>{" "}
              <code>{e.code}</code> {e.message}
            </li>
          ))}
        </ol>
      </div>
    );
  }

  const a = resultado.automaton;
  return (
    <div className="caja ok">
      <h2>Esquema válido ({etiqueta(resultado.format, formato)})</h2>
      <ol className="tuberia" aria-label="Lo que hizo el compilador">
        <li>
          <span className="rotulo">esquema</span>
          <b>{etiqueta(resultado.format, formato)}</b>
        </li>
        <li>
          <span className="rotulo">IR v2</span>
          <b>{resultado.irBytes != null ? `${resultado.irBytes} bytes` : "verificada"}</b>
        </li>
        <li>
          <span className="rotulo">autómata M</span>
          <b>{a.status === "ok" ? `${a.summary.states} estados` : a.status === "limit" ? "límite" : "pendiente"}</b>
        </li>
      </ol>
      <p>{resultado.summary}</p>
      <p className="tenue id">
        id <code>{resultado.id}</code>
      </p>

      <h3>Autómata de pila (DPDA)</h3>
      {a.status === "ok" ? (
        <table className="tabla">
          <tbody>
            {Object.entries(ETIQUETAS).map(([clave, etiqueta]) => (
              <tr key={clave}>
                <th scope="row">{etiqueta}</th>
                <td>{String((a.summary as unknown as Record<string, unknown>)[clave] ?? "—")}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className={a.status === "limit" ? "aviso" : "tenue"}>
          {a.status === "limit" ? "Límite del constructor: " : "Aún no se construye: "}
          {a.message}
        </p>
      )}

      <h3>Representación intermedia (IR v2)</h3>
      {resultado.ir === null ? (
        <p className="tenue">La IR supera el tope de tamaño y el servidor no la incluyó.</p>
      ) : (
        <>
          <button type="button" onClick={() => setVerIr((v) => !v)}>
            {verIr ? "Ocultar IR" : `Ver IR (${resultado.irBytes ?? "?"} bytes)`}
          </button>
          {verIr && <pre className="ir">{resultado.ir}</pre>}
        </>
      )}
    </div>
  );
}
