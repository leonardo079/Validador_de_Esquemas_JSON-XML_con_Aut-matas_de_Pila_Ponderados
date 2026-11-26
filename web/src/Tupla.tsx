import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ResumenAutomata } from "./api";

// La definición formal M = (Q, Σ, Γ, δ, q0, Z0, F, w) del enunciado, interactiva:
// cada símbolo explica qué es y, si hay un esquema compilado, cuánto vale en el
// autómata de ese esquema. Al pasar el ratón o enfocar se muestra; al pulsar
// queda fijo (Esc o un clic fuera lo cierra).

interface Simbolo {
  id: string;
  simbolo: ReactNode;
  nombre: string;
  texto: string;
  valor?: (r: ResumenAutomata) => string;
}

const SIMBOLOS: Simbolo[] = [
  {
    id: "M",
    simbolo: "M",
    nombre: "El autómata",
    texto:
      "Un autómata de pila determinista ponderado: lee el payload byte a byte, una sola vez, con una pila de tamaño acotado, y devuelve un peso (0 válido, k violaciones, ∞ malformado).",
    valor: (r) => `${r.states} estados · ${r.transitions} transiciones`,
  },
  {
    id: "Q",
    simbolo: "Q",
    nombre: "Estados",
    texto:
      "Conjunto finito de estados. Cada uno es un punto del esquema: «esperando una clave», «tras el valor de id», «dentro de un elemento»…",
    valor: (r) => `${r.states} estados`,
  },
  {
    id: "Σ",
    simbolo: "Σ",
    nombre: "Alfabeto de entrada",
    texto:
      "Lo que lee M: las clases de evento que produce el lexer en streaming (abrir objeto, clave conocida, número, cerrar etiqueta…), no los bytes sueltos.",
    valor: (r) => `${r.classes} clases de eventos`,
  },
  {
    id: "Γ",
    simbolo: "Γ",
    nombre: "Alfabeto de pila",
    texto:
      "Lo que M apila: un marco por objeto, arreglo o elemento abierto, con sus contadores y los campos ya vistos. La pila crece con el anidamiento, nunca con la longitud.",
    valor: (r) => `${r.frames} tipos de marco · hasta ${r.maxFrameBytes} B por marco`,
  },
  {
    id: "δ",
    simbolo: "δ",
    nombre: "Función de transición",
    texto:
      "δ(q, a, X) → (q′, operación, peso): con el estado q, el evento a y el tope X, una sola transición posible (determinismo). Apila, desapila o sigue, y suma al peso las violaciones.",
    valor: (r) => `${r.transitions} transiciones`,
  },
  {
    id: "q0",
    simbolo: (
      <>
        q<sub>0</sub>
      </>
    ),
    nombre: "Estado inicial",
    texto: "Donde empieza M antes de leer el primer byte: esperando el valor raíz del payload.",
  },
  {
    id: "Z0",
    simbolo: (
      <>
        Z<sub>0</sub>
      </>
    ),
    nombre: "Fondo de la pila",
    texto:
      "El símbolo inicial de la pila. Si al acabar la entrada solo queda Z₀, todas las estructuras abiertas se cerraron.",
  },
  {
    id: "F",
    simbolo: "F",
    nombre: "Estados de aceptación",
    texto:
      "Si la entrada termina en uno de ellos con la pila en Z₀, el payload está bien formado; su peso dice si cumple el esquema (0) o cuántas violaciones tiene.",
  },
  {
    id: "w",
    simbolo: "w",
    nombre: "Función de peso",
    texto:
      "Lo que hace «ponderado» a M: cada transición lleva un peso del semianillo tropical (ℕ ∪ {∞}, mín, +). Cumplir pesa 0, cada restricción incumplida suma 1 y un malformado o un límite vale ∞. El peso final es el número de violaciones.",
  },
];

export function Tupla({ resumen }: { resumen: ResumenAutomata | null }) {
  const [activo, setActivo] = useState<string | null>(null);
  const [fijo, setFijo] = useState(false);
  const raiz = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!fijo) return;
    const fuera = (e: MouseEvent) => {
      if (!raiz.current?.contains(e.target as Node)) {
        setFijo(false);
        setActivo(null);
      }
    };
    const tecla = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setFijo(false);
        setActivo(null);
      }
    };
    document.addEventListener("mousedown", fuera);
    document.addEventListener("keydown", tecla);
    return () => {
      document.removeEventListener("mousedown", fuera);
      document.removeEventListener("keydown", tecla);
    };
  }, [fijo]);

  const s = SIMBOLOS.find((x) => x.id === activo);
  return (
    <div
      className="tupla-marco"
      ref={raiz}
      onMouseLeave={() => {
        if (!fijo) setActivo(null);
      }}
    >
      <div className="tupla" role="group" aria-label="Definición formal de M: pulsa cada símbolo">
        {SIMBOLOS.map((x, i) => (
          <button
            key={x.id}
            type="button"
            className={i === 0 ? "simbolo primero" : "simbolo"}
            aria-expanded={activo === x.id}
            aria-label={`${x.id}: ${x.nombre}`}
            onMouseEnter={() => !fijo && setActivo(x.id)}
            onFocus={() => !fijo && setActivo(x.id)}
            onBlur={() => !fijo && setActivo(null)}
            onClick={() => {
              if (fijo && activo === x.id) {
                setFijo(false);
                setActivo(null);
              } else {
                setFijo(true);
                setActivo(x.id);
              }
            }}
          >
            {x.simbolo}
            {i === 0 && <span className="igual"> =</span>}
          </button>
        ))}
      </div>
      {s && (
        <div className="tupla-ayuda" role="tooltip">
          <div className="tupla-ayuda-cabeza">
            <span className="tupla-ayuda-simbolo">{s.simbolo}</span>
            <b>{s.nombre}</b>
          </div>
          <p>{s.texto}</p>
          {s.valor && (
            <p className="tupla-ayuda-valor">
              {resumen ? <>En el esquema actual: {s.valor(resumen)}</> : "Compila un esquema para ver su valor."}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
