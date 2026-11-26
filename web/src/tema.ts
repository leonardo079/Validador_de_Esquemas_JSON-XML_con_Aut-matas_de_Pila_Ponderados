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

const TRAZOS_CLARO = ["#1f5fae", "#1d7a5f", "#5b4fb0", "#0e7c8c", "#3d6a99", "#7a5c9e", "#2f7d8a", "#566b84"];
const TRAZOS_OSCURO = ["#6da7ee", "#4cc3a0", "#a99cf0", "#4fc3d4", "#86aedb", "#bb9ee0", "#6cc7c9", "#9fb0c6"];

export function paleta(oscuro: boolean): Paleta {
  return oscuro
    ? {
        panel: "#0f1621",
        tinta: "#e2e8f0",
        grafito: "#8b98aa",
        acento: "#6da7ee",
        ok: "#4cc3a0",
        error: "#f08a75",
        grupos: ["#13233a", "#10291f", "#1f1c3a", "#0f2a30", "#172537", "#251c33", "#122b2e", "#1a222e"],
        trazos: TRAZOS_OSCURO,
      }
    : {
        panel: "#ffffff",
        tinta: "#0f1b2d",
        grafito: "#5d6b7e",
        acento: "#1f5fae",
        ok: "#1d7a5f",
        error: "#c2412d",
        grupos: ["#e9f0f9", "#e5f3ee", "#eeebfa", "#e2f3f5", "#e8eef6", "#f1ebf7", "#e3f2f3", "#edf0f4"],
        trazos: TRAZOS_CLARO,
      };
}
