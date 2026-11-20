import { useEffect, useRef, useState } from "react";
import cytoscape from "cytoscape";
import { pedirGrafo, type FalloVista, type Grafo } from "./vistas";

// Vista «autómata» (F7.6): el grafo de M de `POST /api/automaton` dibujado con
// Cytoscape. Los estados se colorean por grupo (un grupo por subesquema o
// elemento); el de aceptación lleva borde doble y «↩ marco»/«↩ escalar» son
// los destinos que da la pila. Las etiquetas se dibujan en un lienzo: son
// texto, nunca HTML.

const COLORES = ["#dbeafe", "#dcfce7", "#fef3c7", "#fce7f3", "#e0e7ff", "#ccfbf1", "#fee2e2", "#f3e8ff"];

export function elementos(g: Grafo): cytoscape.ElementDefinition[] {
  const nodos = g.nodes.map((n) => ({
    data: {
      id: n.id,
      label: n.special ? n.label : n.name ? `${n.label}\n${n.name}` : n.label,
      color: n.special ? "#ffffff" : COLORES[(n.group ?? 0) % COLORES.length],
      grupo: n.group != null ? (g.groups[n.group] ?? "") : "",
      nombre: n.name ?? "",
    },
    classes: [n.accept ? "acepta" : "", n.special ? "especial" : "", n.id === g.start ? "inicio" : ""]
      .filter(Boolean)
      .join(" "),
  }));
  const aristas = g.edges.map((e) => ({ data: { id: e.id, source: e.source, target: e.target, label: e.label } }));
  return [...nodos, ...aristas];
}

interface Props {
  esquema: string;
}

export function Automata({ esquema }: Props) {
  const contenedor = useRef<HTMLDivElement | null>(null);
  const [grafo, setGrafo] = useState<Grafo | null>(null);
  const [error, setError] = useState<FalloVista | null>(null);
  const [cargando, setCargando] = useState(false);
  const [elegido, setElegido] = useState<string>("");

  const cargar = async () => {
    setCargando(true);
    setError(null);
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
    const cy = cytoscape({
      container: contenedor.current,
      elements: elementos(grafo),
      wheelSensitivity: 0.2,
      style: [
        {
          selector: "node",
          style: {
            label: "data(label)",
            "text-wrap": "wrap",
            "text-max-width": "120px",
            "font-size": 9,
            "text-valign": "center",
            "background-color": "data(color)",
            "border-width": 1,
            "border-color": "#475569",
            width: 54,
            height: 54,
          },
        },
        { selector: "node.acepta", style: { "border-width": 4, "border-style": "double" } },
        { selector: "node.inicio", style: { "border-color": "#16a34a", "border-width": 3 } },
        { selector: "node.especial", style: { shape: "round-rectangle", width: 70, height: 26, "border-style": "dashed" } },
        {
          selector: "edge",
          style: {
            label: "data(label)",
            "font-size": 7,
            "curve-style": "bezier",
            "target-arrow-shape": "triangle",
            "text-rotation": "autorotate",
            "text-background-color": "#ffffff",
            "text-background-opacity": 0.8,
            "line-color": "#94a3b8",
            "target-arrow-color": "#94a3b8",
            width: 1,
          },
        },
        { selector: ":selected", style: { "border-color": "#dc2626", "line-color": "#dc2626", "target-arrow-color": "#dc2626" } },
      ],
      layout: { name: "breadthfirst", directed: true, roots: grafo.start ? [`#${grafo.start}`] : undefined, spacingFactor: 1.2 },
    });
    cy.on("tap", "node, edge", (ev) => {
      const d = ev.target.data();
      setElegido(
        ev.target.isNode()
          ? `${d.id}${d.nombre ? ` — ${d.nombre}` : ""}${d.grupo ? ` (grupo ${d.grupo})` : ""}`
          : `${d.source} → ${d.target}: ${d.label}`,
      );
    });
    return () => cy.destroy();
  }, [grafo]);

  return (
    <div className="vista">
      <div className="barra">
        <button type="button" className="principal" onClick={cargar} disabled={cargando}>
          {cargando ? "Construyendo…" : "Dibujar el autómata del esquema"}
        </button>
        {grafo && (
          <span className="tenue">
            {grafo.states} estados, {grafo.transitions} transiciones ({grafo.format.toUpperCase()})
            {grafo.truncated && " — recortado: se muestran los primeros estados"}
          </span>
        )}
      </div>
      {error && <p className="error">{error.mensaje}</p>}
      {!grafo && !error && <p className="tenue">Compila el esquema del editor y dibuja su DPDA (M).</p>}
      <div ref={contenedor} className="lienzo" aria-label="Grafo del autómata" />
      {elegido && <p className="detalle">{elegido}</p>}
    </div>
  );
}
