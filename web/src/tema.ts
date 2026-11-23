import { useEffect, useState } from "react";

// Tema claro u oscuro del sistema, para lo que no lee el CSS: Monaco (su tema)
// y Cytoscape (dibuja en un lienzo con colores propios). Los colores son los
// mismos tokens de `estilos.css`.

const OSCURO = "(prefers-color-scheme: dark)";

export function useTemaOscuro(): boolean {
  const [oscuro, setOscuro] = useState(() => typeof window !== "undefined" && !!window.matchMedia?.(OSCURO).matches);
  useEffect(() => {
    const consulta = window.matchMedia?.(OSCURO);
    if (!consulta) return;
    const alCambiar = (e: MediaQueryListEvent) => setOscuro(e.matches);
    consulta.addEventListener("change", alCambiar);
    return () => consulta.removeEventListener("change", alCambiar);
  }, []);
  return oscuro;
}

export interface Paleta {
  panel: string;
  tinta: string;
  grafito: string;
  acento: string;
  ok: string;
  error: string;
  /** Relleno de los estados, uno por grupo (subesquema o elemento). */
  grupos: string[];
  /** Borde de los estados, el mismo tono que `grupos` pero saturado. */
  trazos: string[];
}

const TRAZOS_CLARO = ["#0d7377", "#2e7d4f", "#9a6b00", "#a3416e", "#4a55a8", "#167a6e", "#b0452c", "#7048a0"];
const TRAZOS_OSCURO = ["#4cbfba", "#5cc98a", "#e0b04a", "#e07aa6", "#8f9bf0", "#52c7b5", "#ff8a73", "#b896ea"];

export function paleta(oscuro: boolean): Paleta {
  return oscuro
    ? {
        panel: "#13181f",
        tinta: "#e3e8ee",
        grafito: "#95a1b0",
        acento: "#4cbfba",
        ok: "#5cc98a",
        error: "#ff8a73",
        grupos: ["#173338", "#1f3328", "#3a311a", "#3a2230", "#25294a", "#1d3a3a", "#3d2421", "#2f2540"],
        trazos: TRAZOS_OSCURO,
      }
    : {
        panel: "#ffffff",
        tinta: "#18202b",
        grafito: "#5d6877",
        acento: "#0d7377",
        ok: "#2e7d4f",
        error: "#b8321a",
        grupos: ["#e3f1f1", "#e5f2e9", "#f7efd9", "#f5e4ec", "#e6e8f7", "#dff3f0", "#f7e3df", "#ede6f5"],
        trazos: TRAZOS_CLARO,
      };
}
