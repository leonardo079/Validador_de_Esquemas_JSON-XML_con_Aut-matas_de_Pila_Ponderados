import { useEffect, useMemo, useState } from "react";
import { hierarchy, tree, type HierarchyPointNode } from "d3-hierarchy";
import { pedirArbol, type FalloVista, type NodoArbol } from "./vistas";
import { CampoPayload } from "./CampoPayload";
import type { Muestra } from "./ejemplos";

// Vista «árbol de derivación» (F7.6 sobre F5.5): el árbol de
// `POST /api/derivation` con el diseño de `d3-hierarchy` (`tree`) dibujado en
// SVG por React (las etiquetas son nodos de texto: nunca HTML). No terminales
// en elipse, terminales en caja con su lexema y ε en gris.

const ANCHO = 92;
const ALTO = 64;

export interface Diseno {
  nodos: HierarchyPointNode<NodoArbol>[];
  enlaces: { de: HierarchyPointNode<NodoArbol>; a: HierarchyPointNode<NodoArbol> }[];
  ancho: number;
  alto: number;
  minX: number;
}

/** Posiciones de los nodos (puro, para las pruebas). */
export function disenar(raiz: NodoArbol): Diseno {
  const h = tree<NodoArbol>().nodeSize([ANCHO, ALTO])(hierarchy(raiz));
  const nodos = h.descendants();
  let minX = Infinity;
  let maxX = -Infinity;
  let maxY = 0;
  for (const n of nodos) {
    minX = Math.min(minX, n.x);
    maxX = Math.max(maxX, n.x);
    maxY = Math.max(maxY, n.y);
  }
  return {
    nodos,
    enlaces: h.links().map((l) => ({ de: l.source, a: l.target })),
    ancho: maxX - minX + 2 * ANCHO,
    alto: maxY + 2 * ALTO,
    minX,
  };
}

const corto = (s: string, n = 14) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

interface Props {
  payload: string;
  alCambiarPayload: (p: string) => void;
  /** Muestras del ejemplo elegido en «Esquema» (válido, con violaciones, malformado). */
  muestras?: Muestra[];
}

export function Arbol({ payload, alCambiarPayload, muestras }: Props) {
  const [raiz, setRaiz] = useState<NodoArbol | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState(false);
  const [elegido, setElegido] = useState<NodoArbol | null>(null);
  const diseno = useMemo(() => (raiz ? disenar(raiz) : null), [raiz]);

  const cargar = async () => {
    setCargando(true);
    setError(null);
    setElegido(null);
    const r = await pedirArbol(payload);
    setCargando(false);
    if (r.ok) setRaiz(r.datos);
    else {
      setRaiz(null);
      setError(r.fallo);
    }
  };

  // Al abrir la pestaña se deriva el payload actual.
  useEffect(() => {
    void cargar();
  }, []);

  return (
    <div className="vista">
      <CampoPayload payload={payload} alCambiar={alCambiarPayload} muestras={muestras} filas={4} />
      <div className="barra">
        <button type="button" className="principal" onClick={cargar} disabled={cargando}>
          Actualizar el árbol de derivación
        </button>
        {cargando ? <span className="tenue">Derivando…</span> : diseno && <span className="tenue">{diseno.nodos.length} nodos · gramática LL(1) de la Fase 2 · pulsa un nodo para ver sus bytes</span>}
      </div>
      <ul className="leyenda-arbol" aria-label="Leyenda">
        <li>
          <span className="forma no-terminal" /> no terminal (una producción de la gramática)
        </li>
        <li>
          <span className="forma terminal" /> terminal, con su lexema
        </li>
        <li>
          <span className="forma vacio">ε</span> producción vacía
        </li>
      </ul>
      {error && (
        <p className="error">
          {error.mensaje}
          {error.offset != null && ` (byte ${error.offset})`}
        </p>
      )}
      {diseno && (
        <div className="lienzo arbol">
          <svg
            width={diseno.ancho}
            height={diseno.alto}
            viewBox={`${diseno.minX - ANCHO} ${-ALTO / 2} ${diseno.ancho} ${diseno.alto}`}
            role="img"
            aria-label="Árbol de derivación"
          >
            {diseno.enlaces.map((l, i) => (
              <line key={i} x1={l.de.x} y1={l.de.y} x2={l.a.x} y2={l.a.y} className="enlace" />
            ))}
            {diseno.nodos.map((n, i) => {
              const d = n.data;
              return (
                <g key={i} transform={`translate(${n.x},${n.y})`} className={`nodo ${d.kind}`} onClick={() => setElegido(d)}>
                  {d.kind === "nt" ? <ellipse rx={40} ry={14} /> : d.kind === "t" ? <rect x={-40} y={-16} width={80} height={32} rx={4} /> : null}
                  <text y={d.kind === "t" ? -3 : 4} textAnchor="middle">
                    {d.kind === "eps" ? "ε" : corto(d.name)}
                  </text>
                  {d.kind === "t" && d.lexeme !== undefined && d.lexeme !== d.name && (
                    <text y={11} textAnchor="middle" className="lexema">
                      {corto(d.lexeme)}
                    </text>
                  )}
                </g>
              );
            })}
          </svg>
        </div>
      )}
      {elegido && (
        <p className="detalle">
          {elegido.name} — bytes [{elegido.start}, {elegido.end})
          {elegido.lexeme !== undefined && ` — lexema ${JSON.stringify(elegido.lexeme)}`}
          {elegido.pointer !== undefined && ` — puntero «${elegido.pointer || "/"}»`}
        </p>
      )}
    </div>
  );
}
