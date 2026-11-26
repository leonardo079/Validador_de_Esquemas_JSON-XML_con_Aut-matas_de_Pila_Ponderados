import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { compilar, formatoProbable, NOMBRE_FORMATO, type ErrorEsquema, type Formato, type Resultado as R } from "./api";
import { EJEMPLOS, type Ejemplo, type Grupo } from "./ejemplos";
import { Resultado } from "./Resultado";
import { Automata } from "./Automata";
import { Arbol } from "./Arbol";
import { Comparacion } from "./Comparacion";
import { Casos } from "./Casos";
import { Generado } from "./Generado";
import { paleta, useTemaOscuro } from "./tema";
import { Tupla } from "./Tupla";

// Seis vistas, en el orden del recorrido del proyecto: el esquema (F7.4:
// editar, compilar contra POST /api/schemas y ver el resultado), su autómata,
// el árbol de derivación de un payload, la comparación en vivo con la línea
// base, el código generado y los casos del corpus (F7.6, F7.2, F7.3). El
// editor queda montado al cambiar de pestaña (solo se oculta), así Monaco
// conserva el texto y el cursor. Cada vista abre con un encabezado que dice
// para qué sirve.

type Monaco = Parameters<OnMount>[1];

const VISTAS = [
  {
    id: "editor",
    nombre: "Esquema",
    titulo: "Escribe o elige un esquema",
    proposito:
      "El compilador lo analiza, lo traduce a una representación intermedia (IR) y construye M: el autómata de pila que después valida los payloads byte a byte.",
  },
  {
    id: "automata",
    nombre: "Autómata",
    titulo: "El autómata M del esquema",
    proposito:
      "Cada círculo es un estado; cada flecha, lo que M hace al leer una clase de token: avanzar, apilar o desapilar un marco. Pulsa un estado para ver sus transiciones.",
  },
  {
    id: "arbol",
    nombre: "Árbol de derivación",
    titulo: "Cómo se lee el payload",
    proposito:
      "La gramática LL(1) deriva el payload símbolo a símbolo. Es la estructura que M recorre con su pila, sin construirla nunca en memoria.",
  },
  {
    id: "comparacion",
    nombre: "Comparación en vivo",
    titulo: "M frente a un validador con árbol",
    proposito:
      "El mismo payload en los dos validadores: veredicto, peso (número de violaciones), tiempo, memoria y asignaciones, medidos en el servidor en este momento.",
  },
  {
    id: "generado",
    nombre: "Código generado",
    titulo: "M convertido en código",
    proposito:
      "El validador generado en C11 o en Rust: se descarga, se compila y ejecuta en un sandbox aislado, y valida el payload por bloques en streaming.",
  },
  {
    id: "casos",
    nombre: "Casos de prueba",
    titulo: "El corpus de pruebas",
    proposito:
      "Los casos guardados en MongoDB Atlas (válidos, inválidos, malformados, fuera de un DPDA y extremos), cada uno con lo que comprueba, ejecutados contra el DPDA y comparados con lo esperado.",
  },
] as const;
type Vista = (typeof VISTAS)[number]["id"];

const GRUPOS: [Grupo, string][] = [
  ["JSON", "JSON Schema 2020-12"],
  ["XSD", "XSD 1.0"],
  ["DTD", "DTD"],
];

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
        "editorLineNumber.foreground": oscuro ? "#4a5a72" : "#a3b0c2",
        "editorLineNumber.activeForeground": p.tinta,
        "editor.lineHighlightBackground": oscuro ? "#141e2e" : "#f3f6fa",
        "editor.lineHighlightBorder": "#00000000",
        "editor.selectionBackground": oscuro ? "#1e3a5f" : "#d3e3f8",
        "editorCursor.foreground": p.acento,
        "editorIndentGuide.background1": oscuro ? "#1a2536" : "#edf1f6",
      },
    });
  }
}

/**
 * La navegación como la cinta de una máquina: cada vista es una celda con su
 * estado (q0…q5) y el cabezal se desliza a la celda activa. Se coloca con la
 * posición medida de la celda (variables CSS).
 */
function Cinta({ vista, alElegir }: { vista: Vista; alElegir: (v: Vista) => void }) {
  const cinta = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState({ x: 0, w: 0, scroll: 0 });

  const medir = useCallback(() => {
    const c = cinta.current;
    const activa = c?.querySelector<HTMLButtonElement>('[aria-selected="true"]');
    if (!c || !activa) return;
    setPos({ x: activa.offsetLeft, w: activa.offsetWidth, scroll: c.scrollLeft });
  }, []);

  useLayoutEffect(() => {
    medir();
    cinta.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [vista, medir]);

  useEffect(() => {
    window.addEventListener("resize", medir);
    // Las fuentes cambian el ancho de las celdas al cargar.
    document.fonts?.ready.then(medir).catch(() => {});
    return () => window.removeEventListener("resize", medir);
  }, [medir]);

  const estilo = { "--x": `${pos.x}px`, "--w": `${pos.w}px`, "--scroll": `${pos.scroll}px` } as CSSProperties;
  return (
    <div className="cinta-marco" style={estilo}>
      <span className="puntero" aria-hidden="true" />
      <div className="cinta" role="tablist" aria-label="Vistas" ref={cinta} onScroll={medir}>
        <span className="blanco" aria-hidden="true">
          ⊔
        </span>
        <span className="cabezal" aria-hidden="true" />
        {VISTAS.map((v, i) => (
          <button
            key={v.id}
            type="button"
            role="tab"
            className="celda"
            aria-selected={vista === v.id}
            onClick={() => alElegir(v.id)}
          >
            <span className="sim" aria-hidden="true">
              q{i}
            </span>
            <span className="nombre">{v.nombre}</span>
          </button>
        ))}
        <span className="blanco" aria-hidden="true">
          ⊔
        </span>
      </div>
    </div>
  );
}

/** Pila de platos: el símbolo de la casa (el tope, en el color de acento). */
function Logo() {
  return (
    <svg className="logo" viewBox="0 0 40 40" aria-hidden="true">
      <rect className="plato" x="7" y="29" width="26" height="6.5" rx="2" />
      <rect className="plato" x="7" y="20.5" width="26" height="6.5" rx="2" />
      <rect className="plato tope" x="7" y="12" width="26" height="6.5" rx="2" />
      <path className="flecha" d="M20 2v6.5M16.8 5.6 20 8.8l3.2-3.2" />
    </svg>
  );
}

/** Encabezado de cada vista: dónde está uno y para qué sirve. */
function Encabezado({ vista }: { vista: Vista }) {
  const i = VISTAS.findIndex((v) => v.id === vista);
  const v = VISTAS[i];
  return (
    <header className="encabezado-vista">
      <span className="paso">
        q{i} · {v.nombre}
      </span>
      <h2>{v.titulo}</h2>
      <p>{v.proposito}</p>
    </header>
  );
}

const INICIAL = EJEMPLOS.find((e) => e.nombre === "Objeto con requerido") ?? EJEMPLOS[0];

export function App() {
  const [vista, setVista] = useState<Vista>("editor");
  const [ejemplo, setEjemplo] = useState<Ejemplo | null>(INICIAL);
  const [payload, setPayload] = useState(INICIAL.muestras[0]?.payload ?? "");
  const [texto, setTexto] = useState(INICIAL.texto);
  const [resultado, setResultado] = useState<R | null>(null);
  const [cargando, setCargando] = useState(false);
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const formato = formatoProbable(texto);
  const [formatoCompilado, setFormatoCompilado] = useState<Formato | undefined>(undefined);
  const [servicios, setServicios] = useState<{ database: boolean; sandbox: boolean } | null>(null);
  const oscuro = useTemaOscuro();

  // Estado del servidor para la barra de estado (`GET /api/health`).
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

  /** Compila `fuente` (o lo que hay en el editor). */
  const compilarTexto = useCallback(
    async (fuente?: string) => {
      setCargando(true);
      const f = fuente ?? editorRef.current?.getValue() ?? texto;
      const r = await compilar(f);
      // La API dice "xml" para XSD y DTD (D86): la etiqueta sale de la detección.
      setFormatoCompilado(formatoProbable(f));
      setResultado(r);
      setCargando(false);
    },
    [texto],
  );

  // El ejemplo inicial se compila al abrir la página.
  useEffect(() => {
    void compilarTexto(INICIAL.texto);
  }, []);

  /** Elegir un ejemplo: su esquema al editor, su payload válido y compilar. */
  const elegirEjemplo = (e: Ejemplo) => {
    setEjemplo(e);
    setTexto(e.texto);
    if (e.muestras[0]) setPayload(e.muestras[0].payload);
    setResultado(null);
    void compilarTexto(e.texto);
  };

  /** Para la prueba de carga de la comparación: cambia el esquema y su ejemplo. */
  const cambiarEsquema = (t: string) => {
    setTexto(t);
    setEjemplo(EJEMPLOS.find((e) => e.texto === t) ?? null);
    setResultado(null);
  };

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
    setEjemplo(null);
    setResultado(null);
  };

  const muestras = ejemplo?.muestras ?? [];
  // El autómata del último esquema compilado, para la tupla de la cabecera.
  const resumen = resultado?.tipo === "ok" && resultado.automaton.status === "ok" ? resultado.automaton.summary : null;
  return (
    <div className="app">
      <header className="cabecera">
        <div className="marca-proyecto">
          <Logo />
          <div>
            <h1>Validador de esquemas</h1>
            <p>
              JSON Schema, XSD y DTD compilados a un <strong>autómata de pila determinista ponderado</strong> que valida
              en streaming, con memoria fija.
            </p>
          </div>
        </div>
        <Tupla resumen={resumen} />
      </header>

      <nav aria-label="Vistas">
        <Cinta vista={vista} alElegir={setVista} />
      </nav>

      <Encabezado vista={vista} />

      {vista === "automata" && <Automata esquema={texto} />}
      {vista === "arbol" && <Arbol payload={payload} alCambiarPayload={setPayload} muestras={muestras} />}
      {vista === "comparacion" && (
        <Comparacion
          esquema={texto}
          payload={payload}
          alCambiarPayload={setPayload}
          alCambiarEsquema={cambiarEsquema}
          muestras={muestras}
        />
      )}
      {vista === "generado" && (
        <Generado esquema={texto} payload={payload} alCambiarPayload={setPayload} muestras={muestras} />
      )}
      {vista === "casos" && <Casos />}

      <main className={vista === "editor" ? "paneles" : "paneles oculto"}>
        <aside className="galeria" aria-label="Ejemplos">
          {GRUPOS.map(([g, largo]) => (
            <section key={g} className="grupo-ejemplos">
              <h3>
                {g} <span>{largo}</span>
              </h3>
              <ul>
                {EJEMPLOS.filter((e) => e.grupo === g).map((e) => (
                  <li key={e.grupo + e.nombre}>
                    <button
                      type="button"
                      className="ejemplo"
                      aria-pressed={ejemplo === e}
                      onClick={() => elegirEjemplo(e)}
                      title={e.descripcion}
                    >
                      <b>{e.nombre}</b>
                      <span>{e.descripcion}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
          <label className="archivo">
            Abrir un archivo…
            <input type="file" accept=".json,.xsd,.dtd,.xml" onChange={(ev) => alCargarArchivo(ev.target.files?.[0])} />
          </label>
        </aside>

        <section className="editor" aria-label="Editor del esquema">
          <div className="editor-cabeza">
            <span className="rotulo">
              {ejemplo ? `${ejemplo.grupo} · ${ejemplo.nombre}` : "Esquema propio"}
            </span>
            <span className="ficha-formato">{NOMBRE_FORMATO[formato]}</span>
            <button id="compilar" type="button" className="principal" onClick={() => compilarTexto()} disabled={cargando}>
              {cargando ? "Compilando…" : "Compilar"}
              <kbd>Ctrl ↵</kbd>
            </button>
          </div>
          <div className="editor-cuerpo">
            <Editor
              height="100%"
              language={formato === "json" ? "json" : "xml"}
              value={texto}
              onChange={(v) => setTexto(v ?? "")}
              beforeMount={definirTemas}
              onMount={alMontar}
              theme={oscuro ? "validador-oscuro" : "validador-claro"}
              options={{
                minimap: { enabled: false },
                fontSize: 13.5,
                lineHeight: 22,
                fontFamily: '"IBM Plex Mono", ui-monospace, Consolas, monospace',
                scrollBeyondLastLine: false,
                tabSize: 2,
                padding: { top: 12 },
                renderLineHighlight: "line",
                // Al volver a la pestaña del editor, Monaco recalcula su tamaño.
                automaticLayout: true,
              }}
            />
          </div>
        </section>

        <section className="resultado" aria-live="polite">
          <Resultado resultado={resultado} cargando={cargando} alElegirError={irAError} formato={formatoCompilado} />
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
        <span className="derecha">peso: 0 válido · k violaciones · ∞ malformado</span>
      </footer>
    </div>
  );
}
