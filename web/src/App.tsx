import { useCallback, useEffect, useRef, useState } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { compilar, formatoProbable, type ErrorEsquema, type Resultado as R } from "./api";
import { EJEMPLOS } from "./ejemplos";
import { Resultado } from "./Resultado";

// Vista «editor de esquema» (F7.4): editar, compilar contra POST /api/schemas
// (F7.1) y ver el resultado. Las vistas del autómata, del árbol de derivación y
// el panel de métricas son de F7.6.

type Monaco = Parameters<OnMount>[1];

export function App() {
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

      <div className="barra">
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

      <main className="paneles">
        <section className="editor" aria-label="Editor del esquema">
          <Editor
            height="100%"
            language={formato}
            value={texto}
            onChange={(v) => setTexto(v ?? "")}
            onMount={alMontar}
            options={{ minimap: { enabled: false }, fontSize: 14, scrollBeyondLastLine: false, tabSize: 2 }}
          />
        </section>
        <section className="resultado" aria-live="polite">
          <Resultado resultado={resultado} cargando={cargando} alElegirError={irAError} />
        </section>
      </main>
    </div>
  );
}
