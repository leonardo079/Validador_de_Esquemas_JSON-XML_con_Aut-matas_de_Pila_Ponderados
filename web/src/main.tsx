import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./monaco";
import { App } from "./App";
// Fuentes empaquetadas con la app (la CSP no admite otro origen): Manrope
// (variable) para la interfaz y las cifras, e IBM
// Plex Mono para el código, los bytes y los punteros.
import "@fontsource-variable/manrope/wght.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./estilos.css";

createRoot(document.getElementById("raiz")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
