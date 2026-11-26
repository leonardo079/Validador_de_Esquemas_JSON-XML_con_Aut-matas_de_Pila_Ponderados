import { useMemo, useState } from "react";
import { cargaDeUsuarios, LISTA_DE_USUARIOS, type Muestra } from "./ejemplos";
import { CampoPayload } from "./CampoPayload";
import {
  bytes,
  duracion,
  pedirComparacion,
  type Comparacion as C,
  type FalloVista,
  type Medicion,
  type Medido,
} from "./vistas";

// Vista «comparación en vivo» (F7.6; enunciado, objetivo 3): valida el payload
// con el DPDA (M sobre `runtime-rs`) y con la línea base DOM (`serde_json` +
// `jsonschema`, D48) mediante `POST /api/compare`. Muestra:
//   1. el banco de pruebas: lo que el servidor midió ahora mismo con el
//      binario `medir` (mediana del tiempo, MB/s, ns/byte, pico de memoria y
//      asignaciones, con `dhat`);
//   2. el perfil de la pila: la profundidad de anidamiento al leer cada byte,
//      calculada aquí, con el byte del primer error marcado;
//   3. la tabla de veredictos y primer error de cada validador.
// Los números del corpus y del bombeo están en `docs/fase6/metricas.md`.

function veredicto(valido: boolean, kind: string | null): string {
  if (valido) return "válido";
  if (kind === "lexical" || kind === "syntax" || kind === "malformed") return "malformado";
  if (kind === "limit") return "límite superado";
  return "inválido";
}

/** El veredicto con el color de su clase (los mismos que los casos de prueba). */
function Pastilla({ texto }: { texto: string }) {
  const clase = texto === "válido" ? "valido" : texto === "inválido" ? "invalido" : texto === "malformado" ? "malformado" : "limite";
  return <span className={`pastilla ${clase}`}>{texto}</span>;
}

const entero = (n: number) => Math.round(n).toLocaleString("es");
const decimal = (n: number, d = 1) => n.toLocaleString("es", { minimumFractionDigits: d, maximumFractionDigits: d });

/** Cuántas veces más: «185×», «3,2×». */
function veces(a: number, b: number): string {
  const r = a / Math.max(b, 1e-12);
  return r >= 10 ? `${entero(r)}×` : `${decimal(r)}×`;
}

// ── Perfil de la pila ──────────────────────────────────────────────────────

export interface Perfil {
  /** Profundidad máxima de cada tramo de bytes (una columna por tramo). */
  columnas: number[];
  maximo: number;
  total: number;
}

/**
 * Profundidad de anidamiento al leer cada byte (JSON: `{` y `[`; XML: cada
 * etiqueta de apertura sin `/>`), agrupada en `n` columnas con el máximo de
 * cada una. Un recorrido, sin árbol: lo mismo que hace crecer la pila de M.
 */
export function perfilDePila(payload: string, n = 480): Perfil {
  const b = new TextEncoder().encode(payload);
  const total = b.length;
  const cols = Math.max(1, Math.min(n, total));
  const columnas = new Array<number>(cols).fill(0);
  const xml = /^[ \t\r\n]*</.test(payload);
  let d = 0;
  let maximo = 0;
  let cadena = false;
  let escape = false;
  for (let i = 0; i < total; i++) {
    const c = b[i];
    if (xml) {
      if (c === 0x3c /* < */) {
        const s = b[i + 1];
        if (s === 0x2f /* / */) d = Math.max(0, d - 1);
        else if (s !== 0x3f /* ? */ && s !== 0x21 /* ! */) {
          // apertura: busca su `>` para saber si es `/>`
          let j = i + 1;
          while (j < total && b[j] !== 0x3e) j++;
          if (b[j - 1] !== 0x2f) d++;
        }
      }
    } else if (cadena) {
      if (escape) escape = false;
      else if (c === 0x5c) escape = true;
      else if (c === 0x22) cadena = false;
    } else if (c === 0x22) cadena = true;
    else if (c === 0x7b || c === 0x5b) d++;
    else if (c === 0x7d || c === 0x5d) d = Math.max(0, d - 1);
    const k = Math.floor((i * cols) / total);
    if (d > columnas[k]) columnas[k] = d;
    if (d > maximo) maximo = d;
  }
  return { columnas, maximo, total };
}

function PerfilPila({ payload, errorEn }: { payload: string; errorEn: number | null }) {
  const p = useMemo(() => perfilDePila(payload), [payload]);
  if (p.total === 0) return null;
  const alto = Math.max(1, p.maximo);
  const n = p.columnas.length;
  // Área escalonada en un lienzo de n × alto (el SVG se estira al contenedor).
  let d = `M0 ${alto}`;
  p.columnas.forEach((v, i) => {
    d += ` L${i} ${alto - v} L${i + 1} ${alto - v}`;
  });
  d += ` L${n} ${alto} Z`;
  const guias = alto <= 12 ? Array.from({ length: alto - 1 }, (_, i) => i + 1) : [Math.round(alto / 2)];
  const pos = errorEn != null ? Math.min(100, (100 * errorEn) / p.total) : null;
  return (
    <section className="perfil" aria-label="Perfil de la pila">
      <div className="perfil-cabeza">
        <h2>Perfil de la pila</h2>
        <span className="tenue">
          anidamiento al leer cada byte: máximo <b>{p.maximo}</b> en {bytes(p.total)}. La memoria de M es O(d), no O(n).
        </span>
      </div>
      <div className="perfil-grafico">
        <svg viewBox={`0 0 ${n} ${alto}`} preserveAspectRatio="none" role="img" aria-label={`Profundidad máxima ${p.maximo}`}>
          {guias.map((g) => (
            <line key={g} className="guia" x1={0} x2={n} y1={alto - g} y2={alto - g} />
          ))}
          <path className="area" d={d} />
        </svg>
        <span className="perfil-eje-y" style={{ top: 0 }}>
          {p.maximo}
        </span>
        <span className="perfil-eje-y" style={{ top: "100%" }}>
          0
        </span>
        {pos != null && (
          <div className={`perfil-error ${pos > 70 ? "derecha" : "izquierda"}`} style={{ left: `${pos}%` }}>
            <span>primer error · byte {entero(errorEn!)}</span>
          </div>
        )}
      </div>
      <div className="perfil-eje-x">
        <span>byte 0</span>
        <span>{entero(p.total)}</span>
      </div>
    </section>
  );
}

// ── Banco de pruebas en vivo ───────────────────────────────────────────────

/** Una métrica, DPDA frente a línea base, en escala lineal o logarítmica. */
function Medidor(props: {
  nombre: string;
  nota: string;
  dpda: number;
  lb: number | null;
  ver: (v: number) => string;
  log?: boolean;
}) {
  const { dpda, lb, ver, log } = props;
  const escala = (v: number) => (log ? Math.log10(v + 1) : v);
  const max = Math.max(escala(dpda), lb == null ? 0 : escala(lb), 1e-12);
  const ancho = (v: number) => `${(100 * escala(v)) / max}%`;
  return (
    <div className="medidor">
      <div className="medidor-nombre">
        {props.nombre}
        <span>{props.nota}</span>
      </div>
      <div className="pista dpda">
        <span className="quien">DPDA</span>
        <div className="riel">
          <div className="relleno" style={{ width: ancho(dpda) }} />
        </div>
        <span className="valor">{ver(dpda)}</span>
      </div>
      {lb != null && (
        <div className="pista lb">
          <span className="quien">DOM</span>
          <div className="riel">
            <div className="relleno" style={{ width: ancho(lb) }} />
          </div>
          <span className="valor">{ver(lb)}</span>
        </div>
      )}
    </div>
  );
}

function Lecturas({ d, lb }: { d: Medido; lb: Medido | null }) {
  if (!lb) {
    return (
      <dl className="lecturas">
        <div className="lectura">
          <dt>Pico de memoria</dt>
          <dd>
            <span className="cifra">{bytes(d.picoBytes)}</span>
            <span className="pie">fijos: no dependen del tamaño del payload</span>
          </dd>
        </div>
        <div className="lectura">
          <dt>Asignaciones</dt>
          <dd>
            <span className="cifra">{entero(d.asignaciones)}</span>
            <span className="pie">en toda la validación</span>
          </dd>
        </div>
        <div className="lectura">
          <dt>Rendimiento</dt>
          <dd>
            <span className="cifra">
              {decimal(d.mbPorSegundo)}
              <small>MB/s</small>
            </span>
            <span className="pie">{decimal(d.nsPorByte)} ns por byte</span>
          </dd>
        </div>
      </dl>
    );
  }
  const menosMemoria = lb.picoBytes > d.picoBytes;
  return (
    <dl className="lecturas">
      <div className="lectura">
        <dt>Pico de memoria</dt>
        <dd>
          {menosMemoria ? (
            <span className="cifra">
              <span className="marca">{veces(lb.picoBytes, d.picoBytes)}</span>
              <small>menos</small>
            </span>
          ) : (
            <span className="cifra">{bytes(d.picoBytes)}</span>
          )}
          <span className="pie">
            {bytes(d.picoBytes)} del DPDA frente a {bytes(lb.picoBytes)} del árbol DOM
            {!menosMemoria && ": con un payload pequeño el árbol aún cabe; prueba la carga grande"}
          </span>
        </dd>
      </div>
      <div className="lectura">
        <dt>Asignaciones</dt>
        <dd>
          <span className="cifra">
            {entero(lb.asignaciones)} <small>→ {entero(d.asignaciones)}</small>
          </span>
          <span className="pie">reservas de memoria de la línea base frente a las del DPDA</span>
        </dd>
      </div>
      <div className="lectura">
        <dt>Rendimiento</dt>
        <dd>
          <span className="cifra">
            {decimal(d.mbPorSegundo)}
            <small>MB/s</small>
          </span>
          <span className="pie">
            {d.mbPorSegundo >= lb.mbPorSegundo
              ? `${veces(d.mbPorSegundo, lb.mbPorSegundo)} la velocidad de la línea base (${decimal(lb.mbPorSegundo)} MB/s)`
              : `línea base: ${decimal(lb.mbPorSegundo)} MB/s`}
          </span>
        </dd>
      </div>
    </dl>
  );
}

function Banco({ m }: { m: Medicion }) {
  if (!m.disponible) return <p className="tenue">Medición en vivo: {m.motivo}.</p>;
  const { dpda: d, lineaBase: lb } = m;
  return (
    <section className="banco" aria-label="Banco de pruebas en vivo">
      <div className="banco-titulo">
        <h2>
          <span className="en-vivo" aria-hidden="true" />
          Banco de pruebas en vivo
        </h2>
        <span className="rotulo">
          {bytes(m.bytes)} · profundidad {m.profundidad} · mediana de {d.repeticiones} repeticiones
        </span>
      </div>
      <Lecturas d={d} lb={lb} />
      <div className="medidores">
        <Medidor nombre="Pico de memoria" nota="escala logarítmica" dpda={d.picoBytes} lb={lb?.picoBytes ?? null} ver={bytes} log />
        <Medidor nombre="Asignaciones" nota="escala logarítmica" dpda={d.asignaciones} lb={lb?.asignaciones ?? null} ver={entero} log />
        <Medidor nombre="Tiempo por byte" nota="menos es mejor" dpda={d.nsPorByte} lb={lb?.nsPorByte ?? null} ver={(v) => `${decimal(v)} ns/B`} />
        <Medidor nombre="Validar el payload" nota="mediana" dpda={d.segundos * 1e9} lb={lb ? lb.segundos * 1e9 : null} ver={duracion} />
      </div>
      <p className="banco-nota">
        Medido en el servidor ahora mismo, en un proceso aparte con <code>dhat</code> como asignador.
        {lb ? " DOM: serde_json + jsonschema, que construyen el árbol del payload antes de validar." : ` ${m.motivoLineaBase ?? ""}`}
      </p>
    </section>
  );
}

// ── Vista ──────────────────────────────────────────────────────────────────

interface Props {
  esquema: string;
  payload: string;
  alCambiarPayload: (p: string) => void;
  /** Muestras del ejemplo elegido en «Esquema» (válido, con violaciones, malformado). */
  muestras?: Muestra[];
  /** Para «prueba de carga»: pone el esquema de la lista de usuarios. */
  alCambiarEsquema?: (e: string) => void;
}

const CARGA = 10_000;

export function Comparacion({ esquema, payload, alCambiarPayload, alCambiarEsquema, muestras }: Props) {
  const [r, setR] = useState<C | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState(false);
  // El payload grande no se pinta entero en el área de texto.
  const grande = payload.length > 200_000;

  const validar = async (e = esquema, p = payload) => {
    setCargando(true);
    setError(null);
    const x = await pedirComparacion(e, p);
    setCargando(false);
    if (x.ok) setR(x.datos);
    else {
      setR(null);
      setError(x.fallo);
    }
  };

  const pruebaDeCarga = () => {
    const p = cargaDeUsuarios(CARGA);
    alCambiarEsquema?.(LISTA_DE_USUARIOS);
    alCambiarPayload(p);
    void validar(LISTA_DE_USUARIOS, p);
  };

  const lb = r?.baseline;
  return (
    <div className="vista">
      {grande ? (
        <div className="campo-payload">
          <span className="rotulo">Payload</span>
          <p className="aviso-carga">
            Prueba de carga: {entero(CARGA)} objetos, {bytes(new TextEncoder().encode(payload).length)}, uno de ellos con{" "}
            <code>"id": 0</code> (viola <code>minimum: 1</code>). El esquema del editor es ahora «Lista de usuarios».{" "}
            <button type="button" className="enlace" onClick={() => alCambiarPayload(muestras?.[0]?.payload ?? "[]")}>
              volver a un payload corto
            </button>
          </p>
        </div>
      ) : (
        <CampoPayload payload={payload} alCambiar={alCambiarPayload} muestras={muestras} filas={5} />
      )}
      <div className="barra">
        <button type="button" className="principal" onClick={() => validar()} disabled={cargando}>
          {cargando ? "Midiendo…" : "Validar con el DPDA y la línea base"}
        </button>
        {alCambiarEsquema && (
          <button type="button" onClick={pruebaDeCarga} disabled={cargando}>
            Prueba de carga: {entero(CARGA)} objetos
          </button>
        )}
        {r && (
          <span className={r.agree === false ? "error" : "tenue"}>
            {r.agree === null ? "sin línea base para comparar" : r.agree ? "veredictos: coinciden" : "¡los veredictos no coinciden!"} ·{" "}
            {bytes(r.payloadBytes)} de payload
          </span>
        )}
      </div>
      {error && <p className="error">{error.mensaje}</p>}
      {r && (
        <>
          <Banco m={r.medicion} />
          <PerfilPila payload={payload} errorEn={r.dpda.offset} />
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
                <td>
                  <Pastilla texto={veredicto(r.dpda.valid, r.dpda.errorKind)} />
                </td>
                <td>{lb?.disponible ? <Pastilla texto={veredicto(lb.valid, lb.kind)} /> : "—"}</td>
              </tr>
              <tr>
                <th>Violaciones (peso)</th>
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
                <th>Validar (una pasada)</th>
                <td>{duracion(r.dpda.validateNs)}</td>
                <td>{lb?.disponible ? duracion(lb.validateNs) : "—"}</td>
              </tr>
              <tr>
                <th>Memoria de validación</th>
                <td>{bytes(r.dpda.memoryBytes)} fijos (pila acotada)</td>
                <td>
                  {!lb?.disponible
                    ? "—"
                    : r.medicion.disponible && r.medicion.lineaBase
                      ? `${bytes(r.medicion.lineaBase.picoBytes)} de pico: árbol DOM`
                      : "árbol DOM: crece con el payload"}
                </td>
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
        </>
      )}
    </div>
  );
}
