import { useCallback, useEffect, useRef, useState } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { compilar, formatoProbable, type ErrorEsquema, type Resultado as R } from "./api";
import { EJEMPLOS } from "./ejemplos";
import { Resultado } from "./Resultado";
import { Automata } from "./Automata";
import { Arbol } from "./Arbol";
import { Comparacion } from "./Comparacion";

// Cuatro vistas: «editor de esquema» (F7.4: editar, compilar contra
// POST /api/schemas y ver el resultado) y, desde F7.6, «autómata» (Cytoscape),
// «árbol de derivación» (d3-hierarchy) y «comparación» DPDA frente a la línea
// base. El editor queda montado al cambiar de pestaña (solo se oculta), así
// Monaco conserva el texto y el cursor.

type Monaco = Parameters<OnMount>[1];

const VISTAS = [
  ["editor", "Editor de esquema"],
  ["automata", "Autómata"],
  ["arbol", "Árbol de derivación"],
  ["comparacion", "Comparación DPDA / línea base"],
] as const;
type Vista = (typeof VISTAS)[number][0];

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
      <header>
        <h1>Validador de esquemas JSON/XML con autómatas de pila ponderados</h1>
        <p className="tenue">
          Edita un JSON Schema (subconjunto de 2020-12) o un XSD (subconjunto de 1.0) y compílalo a su DPDA.
        </p>
      </header>

      <nav className="pestanas" role="tablist" aria-label="Vistas">
        {VISTAS.map(([id, nombre]) => (
          <button key={id} type="button" role="tab" aria-selected={vista === id} onClick={() => setVista(id)}>
            {nombre}
          </button>
        ))}
      </nav>

      {vista === "automata" && <Automata esquema={texto} />}
      {vista === "arbol" && <Arbol payload={payload} alCambiarPayload={setPayload} />}
      {vista === "comparacion" && <Comparacion esquema={texto} payload={payload} alCambiarPayload={setPayload} />}

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
            onMount={alMontar}
            options={{
              minimap: { enabled: false },
              fontSize: 14,
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
    </div>
  );
}
