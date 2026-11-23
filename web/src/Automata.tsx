import { useEffect, useRef, useState } from "react";
import cytoscape from "cytoscape";
import dagre from "cytoscape-dagre";
import { pedirGrafo, type FalloVista, type Grafo } from "./vistas";
import { paleta, useTemaOscuro, type Paleta } from "./tema";

// Vista «autómata» (F7.6): el grafo de M de `POST /api/automaton` dibujado con
// Cytoscape. Para que 50 estados y cientos de transiciones se lean:
// - capas de izquierda a derecha (dagre), como se dibujan los autómatas;
// - las transiciones paralelas (mismo origen y destino, distinta clase de
//   evento) van en una sola arista, más gruesa cuantas más junta;
// - las etiquetas de las aristas solo se ven al pulsar un estado, que además
//   resalta sus transiciones y atenúa el resto.
// Los estados llevan el color de su grupo (subesquema o elemento); el de
// aceptación, borde doble; «↩ marco»/«↩ escalar» son los destinos que da la
// pila. Todo se dibuja en un lienzo: el texto nunca se interpreta como HTML.

cytoscape.use(dagre);

/** Cuántas clases de evento se nombran en una arista agrupada. */
const MAX_ETIQUETAS = 4;

export function elementos(g: Grafo, p: Paleta = paleta(false)): cytoscape.ElementDefinition[] {
  const nodos = g.nodes.map((n) => ({
    data: {
      id: n.id,
      label: n.special ? n.label : n.name ? `${n.label}\n${n.name}` : n.label,
      color: n.special ? p.panel : p.grupos[(n.group ?? 0) % p.grupos.length],
      trazo: n.special ? p.grafito : p.trazos[(n.group ?? 0) % p.trazos.length],
      grupo: n.group != null ? (g.groups[n.group] ?? "") : "",
      nombre: n.name ?? "",
    },
    classes: [n.accept ? "acepta" : "", n.special ? "especial" : "", n.id === g.start ? "inicio" : ""]
      .filter(Boolean)
      .join(" "),
  }));
  // Una arista por par (origen, destino) con todas sus etiquetas.
  const pares = new Map<string, { id: string; source: string; target: string; etiquetas: string[] }>();
  for (const e of g.edges) {
    const clave = `${e.source}\u0000${e.target}`;
    const par = pares.get(clave);
    if (par) par.etiquetas.push(e.label);
    else pares.set(clave, { id: e.id, source: e.source, target: e.target, etiquetas: [e.label] });
  }
  const aristas = [...pares.values()].map((a) => {
    const resto = a.etiquetas.length - MAX_ETIQUETAS;
    const visibles = a.etiquetas.slice(0, MAX_ETIQUETAS).join("\n");
    return {
      data: {
        id: a.id,
        source: a.source,
        target: a.target,
        label: resto > 0 ? `${visibles}\n+${resto} más` : visibles,
        todas: a.etiquetas.join(" · "),
        cuantas: a.etiquetas.length,
      },
    };
  });
  return [...nodos, ...aristas];
}

interface Props {
  esquema: string;
}

interface Detalle {
  titulo: string;
  lineas: string[];
}

export function Automata({ esquema }: Props) {
  const contenedor = useRef<HTMLDivElement | null>(null);
  const cyRef = useRef<cytoscape.Core | null>(null);
  const [grafo, setGrafo] = useState<Grafo | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState(false);
  const [elegido, setElegido] = useState<Detalle | null>(null);
  const oscuro = useTemaOscuro();

  const cargar = async () => {
    setCargando(true);
    setError(null);
    setElegido(null);
    const r = await pedirGrafo(esquema);
    setCargando(false);
    if (r.ok) setGrafo(r.datos);
    else {
      setGrafo(null);
      setError(r.fallo);
    }
  };

  useEffect(() => {
    if (!grafo || !contenedor.current) return;
    const p = paleta(oscuro);
    const cy = cytoscape({
      container: contenedor.current,
      elements: elementos(grafo, p),
      wheelSensitivity: 0.25,
      minZoom: 0.15,
      maxZoom: 3,
      style: [
        {
          selector: "node",
          style: {
            label: "data(label)",
            "text-wrap": "wrap",
            "text-max-width": "110px",
            "font-size": 11,
            "font-family": '"IBM Plex Sans", system-ui, sans-serif',
            color: p.tinta,
            "text-valign": "bottom",
            "text-margin-y": 4,
            "text-background-color": p.panel,
            "text-background-opacity": 0.75,
            "text-background-padding": "1px",
            "background-color": "data(color)",
            "border-width": 2,
            "border-color": "data(trazo)",
            width: 34,
            height: 34,
          },
        },
        { selector: "node.acepta", style: { "border-width": 6, "border-style": "double", "border-color": p.acento } },
        { selector: "node.inicio", style: { "border-color": p.ok, "border-width": 3.5 } },
        {
          selector: "node.especial",
          style: { shape: "round-rectangle", width: 64, height: 24, "border-style": "dashed", "text-valign": "center", "text-margin-y": 0 },
        },
        {
          selector: "edge",
          style: {
            "curve-style": "bezier",
            "control-point-step-size": 24,
            "target-arrow-shape": "triangle",
            "arrow-scale": 0.8,
            "line-color": p.grafito,
            "target-arrow-color": p.grafito,
            opacity: 0.45,
            width: "mapData(cuantas, 1, 30, 1, 4)",
          },
        },
        { selector: "edge[source = target]", style: { "loop-direction": "-45deg", "loop-sweep": "40deg" } },
        // Al elegir un estado: lo suyo resaltado (con etiquetas), lo demás atenuado.
        { selector: ".atenuado", style: { opacity: 0.08, "text-opacity": 0.15 } },
        {
          selector: "edge.foco",
          style: {
            opacity: 1,
            "line-color": p.acento,
            "target-arrow-color": p.acento,
            label: "data(label)",
            "text-wrap": "wrap",
            "font-size": 9,
            "font-family": '"IBM Plex Mono", ui-monospace, monospace',
            color: p.tinta,
            "text-background-color": p.panel,
            "text-background-opacity": 0.92,
            "text-background-padding": "2px",
            "z-index": 10,
          },
        },
        { selector: "node.foco", style: { "border-color": p.acento, "z-index": 10 } },
      ],
      layout: {
        name: "dagre",
        rankDir: "LR",
        nodeSep: 22,
        rankSep: 70,
        edgeSep: 8,
        padding: 24,
        animate: false,
      } as cytoscape.LayoutOptions,
    });
    cyRef.current = cy;

    // Acerca la vista a lo elegido (sin animación si el sistema pide menos movimiento).
    const quieto = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    const encuadrar = (eles?: cytoscape.CollectionArgument) => {
      cy.stop(true, false); // la animación nueva sustituye a las pendientes
      cy.animate({ fit: { eles: eles ?? cy.elements(), padding: eles ? 90 : 24 } }, { duration: quieto ? 0 : 280 });
    };
    const limpiar = () => {
      if (cy.$(".foco").empty()) return;
      cy.elements().removeClass("atenuado foco");
      setElegido(null);
      encuadrar();
    };
    cy.on("tap", (ev) => {
      if (ev.target === cy) limpiar();
    });
    cy.on("tap", "node", (ev) => {
      const n = ev.target as cytoscape.NodeSingular;
      const d = n.data();
      const vecindad = n.closedNeighborhood();
      cy.elements().addClass("atenuado").removeClass("foco");
      vecindad.removeClass("atenuado").addClass("foco");
      encuadrar(vecindad);
      const salen = n.outgoers("edge").map((e) => `→ ${e.data("target")}: ${e.data("todas")}`);
      const entran = n.incomers("edge").map((e) => `← ${e.data("source")}: ${e.data("todas")}`);
      setElegido({
        titulo: `${d.id}${d.nombre ? ` — ${d.nombre}` : ""}${d.grupo ? ` (grupo ${d.grupo})` : ""}`,
        lineas: [...salen, ...entran],
      });
    });
    cy.on("tap", "edge", (ev) => {
      const e = ev.target as cytoscape.EdgeSingular;
      cy.elements().addClass("atenuado").removeClass("foco");
      e.union(e.connectedNodes()).removeClass("atenuado").addClass("foco");
      encuadrar(e.union(e.connectedNodes()));
      setElegido({ titulo: `${e.source().id()} → ${e.target().id()}`, lineas: e.data("todas").split(" · ") });
    });
    return () => {
      cyRef.current = null;
      cy.destroy();
    };
  }, [grafo, oscuro]);

  return (
    <div className="vista">
      <div className="barra">
        <button type="button" className="principal" onClick={cargar} disabled={cargando}>
          {cargando ? "Construyendo…" : "Dibujar el autómata del esquema"}
        </button>
        {grafo && (
          <>
            <span className="tenue">
              {grafo.states} estados, {grafo.transitions} transiciones ({grafo.format.toUpperCase()})
              {grafo.truncated && " — recortado: se muestran los primeros estados"}
            </span>
            <button
              type="button"
              onClick={() => {
                const cy = cyRef.current;
                if (!cy) return;
                cy.elements().removeClass("atenuado foco");
                setElegido(null);
                cy.fit(undefined, 24);
              }}
            >
              Encuadrar
            </button>
          </>
        )}
      </div>
      {error && <p className="error">{error.mensaje}</p>}
      {!grafo && !error && <p className="tenue">Compila el esquema del editor y dibuja su DPDA (M).</p>}
      <div className="grafo">
        <div ref={contenedor} className="lienzo" aria-label="Grafo del autómata" />
        {grafo && (
          <aside className="ficha" aria-live="polite">
            {elegido ? (
              <>
                <h3>{elegido.titulo}</h3>
                <ul className="transiciones">
                  {elegido.lineas.map((l, i) => (
                    <li key={i}>{l}</li>
                  ))}
                </ul>
              </>
            ) : (
              <>
                <h3>Cómo leerlo</h3>
                <p className="tenue">Pulsa un estado para ver sus transiciones; pulsa el fondo para volver. La rueda acerca y aleja.</p>
                <ul className="leyenda">
                  <li><span className="muestra inicio" /> estado inicial</li>
                  <li><span className="muestra acepta" /> estado de aceptación</li>
                  <li><span className="muestra especial" /> destino que decide la pila (↩)</li>
                  <li><span className="muestra grupo" /> color: subesquema o elemento</li>
                  <li><span className="muestra arista" /> más gruesa: más clases de evento</li>
                </ul>
              </>
            )}
          </aside>
        )}
      </div>
    </div>
  );
}
