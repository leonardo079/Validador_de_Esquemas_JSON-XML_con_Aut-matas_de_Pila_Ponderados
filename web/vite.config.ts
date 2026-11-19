/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// En desarrollo, `/api` se reenvía al servidor de F7.1 (`cargo run -p server`),
// así la página y la API comparten origen y no hace falta CORS (B9 de F7.1).
// En el despliegue (F7.7) Caddy sirve `dist/` y la API en el mismo origen.
const servidor = process.env.VITE_API_TARGET ?? "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: { "/api": { target: servidor, changeOrigin: false } },
  },
  build: { outDir: "dist", sourcemap: false },
  test: { environment: "jsdom", include: ["src/**/*.test.ts", "src/**/*.test.tsx"] },
});
