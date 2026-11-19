import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./monaco";
import { App } from "./App";
import "./estilos.css";

createRoot(document.getElementById("raiz")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
