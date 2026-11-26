import { NOMBRE_MUESTRA, type Muestra } from "./ejemplos";

// Campo del payload, común a las vistas que validan o derivan: el área de
// texto y, si el ejemplo elegido en «Esquema» las trae, sus muestras (válido,
// con violaciones, malformado) para probar con un clic.

interface Props {
  payload: string;
  alCambiar: (p: string) => void;
  muestras?: Muestra[];
  filas?: number;
}

export function CampoPayload({ payload, alCambiar, muestras = [], filas = 4 }: Props) {
  return (
    <div className="campo-payload">
      <div className="campo-cabeza">
        <label htmlFor="payload" className="rotulo">
          Payload (JSON o XML)
        </label>
        {muestras.length > 0 && (
          <div className="muestras" role="group" aria-label="Payloads de muestra del ejemplo">
            {muestras.map((m) => (
              <button
                key={m.tipo}
                type="button"
                className={`muestra-payload ${m.tipo}`}
                aria-pressed={payload === m.payload}
                onClick={() => alCambiar(m.payload)}
                title={m.payload}
              >
                {NOMBRE_MUESTRA[m.tipo]}
                <span>{m.nota}</span>
              </button>
            ))}
          </div>
        )}
      </div>
      <textarea id="payload" value={payload} onChange={(e) => alCambiar(e.target.value)} rows={filas} spellCheck={false} />
    </div>
  );
}
