import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./monaco";
import { App } from "./App";
// Fuentes empaquetadas con la app (la CSP no admite fuentes de otro origen).
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/jetbrains-mono/400.css";
import "./estilos.css";

createRoot(document.getElementById("raiz")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
