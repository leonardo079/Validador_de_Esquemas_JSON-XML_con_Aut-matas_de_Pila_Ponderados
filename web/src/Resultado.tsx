import { useState } from "react";
import type { ErrorEsquema, Resultado as R } from "./api";

// Panel de resultados. Todo se pinta como texto: React escapa los valores, y no
// se usa `dangerouslySetInnerHTML` en ninguna parte (la IR y los mensajes traen
// texto del usuario sin escapar, `docs/fase7/pendientes.md` §5).

interface Props {
  resultado: R | null;
  cargando: boolean;
  /** Mueve el cursor del editor al error elegido. */
  alElegirError?: (e: ErrorEsquema) => void;
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

export function Resultado({ resultado, cargando, alElegirError }: Props) {
  const [verIr, setVerIr] = useState(false);

  if (cargando) return <p className="estado">Compilando…</p>;
  if (resultado === null) {
    return <p className="estado">Escribe o carga un esquema y pulsa «Compilar».</p>;
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
          Esquema no válido ({resultado.format === "xml" ? "XSD" : "JSON Schema"}):{" "}
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
      <h2>Esquema válido ({resultado.format === "xml" ? "XSD" : "JSON Schema"})</h2>
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
