import { useCallback, useEffect, useRef, useState } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { compilar, formatoProbable, type ErrorEsquema, type Resultado as R } from "./api";
import { EJEMPLOS } from "./ejemplos";
import { Resultado } from "./Resultado";
import { Automata } from "./Automata";
import { Arbol } from "./Arbol";
import { Comparacion } from "./Comparacion";
import { Casos } from "./Casos";
import { Generado } from "./Generado";
import { paleta, useTemaOscuro } from "./tema";

// Cuatro vistas: «editor de esquema» (F7.4: editar, compilar contra
// POST /api/schemas y ver el resultado) y, desde F7.6, «autómata» (Cytoscape),
// «árbol de derivación» (d3-hierarchy) y «comparación» DPDA frente a la línea
// base. El editor queda montado al cambiar de pestaña (solo se oculta), así
// Monaco conserva el texto y el cursor.

type Monaco = Parameters<OnMount>[1];

const VISTAS = [
  ["editor", "Esquema"],
  ["automata", "Autómata"],
  ["arbol", "Árbol de derivación"],
  ["comparacion", "DPDA frente a línea base"],
  ["generado", "Código generado"],
  ["casos", "Casos de prueba"],
] as const;
type Vista = (typeof VISTAS)[number][0];

// Temas de Monaco con el fondo y las líneas de los paneles (`estilos.css`).
function definirTemas(mo: Monaco) {
  for (const oscuro of [false, true]) {
    const p = paleta(oscuro);
    mo.editor.defineTheme(oscuro ? "validador-oscuro" : "validador-claro", {
      base: oscuro ? "vs-dark" : "vs",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": p.panel,
        "editorLineNumber.foreground": p.grafito,
        "editorLineNumber.activeForeground": p.tinta,
        "editor.lineHighlightBackground": oscuro ? "#19212b" : "#f6f7f9",
      },
    });
  }
}

const PAYLOAD_INICIAL = '{"id": 7, "nombre": "Ana", "activo": true}';

export function App() {
  const [vista, setVista] = useState<Vista>("editor");
  const [payload, setPayload] = useState(PAYLOAD_INICIAL);
  const [texto, setTexto] = useState(EJEMPLOS[0].texto);
  const [resultado, setResultado] = useState<R | null>(null);
  const [cargando, setCargando] = useState(false);
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const formato = formatoProbable(texto);
  const [servicios, setServicios] = useState<{ database: boolean; sandbox: boolean } | null>(null);
  const oscuro = useTemaOscuro();

  // Estado del servidor para la cabecera (`GET /api/health`); si falla, no se muestra.
  useEffect(() => {
    let vivo = true;
    fetch("/api/health")
      .then((r) => (r.ok ? r.json() : null))
      .then((v) => {
        if (vivo && v && typeof v === "object")
          setServicios({ database: v.database === true, sandbox: v.sandbox === true });
      })
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, []);

  const alMontar: OnMount = (ed, mo) => {
    editorRef.current = ed;
    monacoRef.current = mo;
    // Ctrl/Cmd + Enter compila.
    ed.addCommand(mo.KeyMod.CtrlCmd | mo.KeyCode.Enter, () => document.getElementById("compilar")?.click());
  };

  // Los errores del servidor se marcan en el editor (subrayado y lista).
  useEffect(() => {
    const ed = editorRef.current;
    const mo = monacoRef.current;
    const modelo = ed?.getModel();
    if (!ed || !mo || !modelo) return;
    const marcas =
      resultado?.tipo === "esquema"
        ? resultado.errors.map((e) => ({
            severity: mo.MarkerSeverity.Error,
            message: `${e.code}: ${e.message}`,
            startLineNumber: e.line,
            startColumn: e.column,
            endLineNumber: e.line,
            endColumn: e.column + 1,
            source: "schemac",
          }))
        : [];
    mo.editor.setModelMarkers(modelo, "schemac", marcas);
  }, [resultado]);

  const alCompilar = useCallback(async () => {
    setCargando(true);
    const r = await compilar(editorRef.current?.getValue() ?? texto);
    setResultado(r);
    setCargando(false);
  }, [texto]);

  const irAError = (e: ErrorEsquema) => {
    const ed = editorRef.current;
    if (!ed) return;
    ed.setPosition({ lineNumber: e.line, column: e.column });
    ed.revealPositionInCenter({ lineNumber: e.line, column: e.column });
    ed.focus();
  };

  const alCargarArchivo = async (archivo: File | undefined) => {
    if (!archivo) return;
    setTexto(await archivo.text());
    setResultado(null);
  };

  return (
    <div className="app">
      <header className="cabecera">
        <div>
          <h1>Validador de esquemas</h1>
          <p>JSON Schema 2020-12 y XSD 1.0, compilados a un autómata de pila determinista ponderado.</p>
        </div>
        <p className="tupla" aria-label="M es la tupla Q, Sigma, Gamma, delta, q cero, Z cero, F">
          M = (Q, Σ, Γ, δ, q<sub>0</sub>, Z<sub>0</sub>, F)
        </p>
      </header>

      {/* Las vistas como diagrama de estados: la activa lleva el doble círculo. */}
      <nav className="estados" role="tablist" aria-label="Vistas">
        {VISTAS.map(([id, nombre], i) => (
          <button key={id} type="button" role="tab" aria-selected={vista === id} onClick={() => setVista(id)}>
            {i > 0 && <span className="delta" aria-hidden="true" />}
            <span className="q" aria-hidden="true" />
            {nombre}
          </button>
        ))}
      </nav>

      {vista === "automata" && <Automata esquema={texto} />}
      {vista === "arbol" && <Arbol payload={payload} alCambiarPayload={setPayload} />}
      {vista === "comparacion" && <Comparacion esquema={texto} payload={payload} alCambiarPayload={setPayload} />}
      {vista === "generado" && <Generado esquema={texto} payload={payload} alCambiarPayload={setPayload} />}
      {vista === "casos" && <Casos />}

      <div className={vista === "editor" ? "barra" : "barra oculto"}>
        <label>
          Ejemplo:{" "}
          <select
            defaultValue=""
            onChange={(ev) => {
              const e = EJEMPLOS.find((x) => x.nombre === ev.target.value);
              if (e) {
                setTexto(e.texto);
                setResultado(null);
              }
              ev.target.value = "";
            }}
          >
            <option value="" disabled>
              elegir…
            </option>
            {EJEMPLOS.map((e) => (
              <option key={e.nombre}>{e.nombre}</option>
            ))}
          </select>
        </label>
        <label className="archivo">
          Abrir archivo…
          <input type="file" accept=".json,.xsd,.xml" onChange={(ev) => alCargarArchivo(ev.target.files?.[0])} />
        </label>
        <span className="tenue">Formato detectado: {formato === "xml" ? "XSD" : "JSON Schema"}</span>
        <button id="compilar" type="button" className="principal" onClick={alCompilar} disabled={cargando}>
          {cargando ? "Compilando…" : "Compilar (Ctrl+Enter)"}
        </button>
      </div>

      <main className={vista === "editor" ? "paneles" : "paneles oculto"}>
        <section className="editor" aria-label="Editor del esquema">
          <Editor
            height="100%"
            language={formato}
            value={texto}
            onChange={(v) => setTexto(v ?? "")}
            beforeMount={definirTemas}
            onMount={alMontar}
            theme={oscuro ? "validador-oscuro" : "validador-claro"}
            options={{
              minimap: { enabled: false },
              fontSize: 14,
              fontFamily: '"IBM Plex Mono", ui-monospace, Consolas, monospace',
              scrollBeyondLastLine: false,
              tabSize: 2,
              // Al volver a la pestaña del editor, Monaco recalcula su tamaño.
              automaticLayout: true,
            }}
          />
        </section>
        <section className="resultado" aria-live="polite">
          <Resultado resultado={resultado} cargando={cargando} alElegirError={irAError} />
        </section>
      </main>

      {/* Barra de estado: lo que el servidor tiene configurado (`/api/health`). */}
      <footer className="barra-estado" aria-label="Estado del servidor">
        {servicios ? (
          <>
            <span className={servicios.database ? "si" : "no"}>
              Casos en MongoDB Atlas: {servicios.database ? "conectado" : "sin conectar"}
            </span>
            <span className={servicios.sandbox ? "si" : "no"}>
              Sandbox de código generado: {servicios.sandbox ? "activo" : "apagado"}
            </span>
          </>
        ) : (
          <span className="no">Servidor sin respuesta</span>
        )}
      </footer>
    </div>
  );
}
