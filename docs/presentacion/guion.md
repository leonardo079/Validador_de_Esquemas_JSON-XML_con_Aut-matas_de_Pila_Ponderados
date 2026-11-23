# Guion de la presentación y de la demo (E7)

Entregable 5 del enunciado: «demo en vivo, explicación de las fases y demostración de los casos de prueba con
métricas comparativas».

- **Diapositivas:** la presentación en claude.ai «Validador JSON/XML con DPDA ponderados», 18 diapositivas. Se
  descarga en PowerPoint o PDF desde Share › Export. Cada diapositiva lleva en sus notas lo que se dice.
- **Este archivo:** el reparto, los tiempos, la demo paso a paso, el video de respaldo, los ensayos y las
  preguntas probables.

Todas las cifras salen de [informe-final.md](../informe-final.md). Si cambia una medida, se cambia allí y aquí.

## 1. Reparto y tiempos

Duración prevista: unos 25 minutos más preguntas, 5 de ellos de demo. Si el profesor fija otra duración, se recorta primero la
diapositiva 9 (recursión) y después la 5 (léxico), y se fusionan la 13 y la 14.

| # | Diapositiva | Quién | Tiempo | Acumulado |
| --- | --- | --- | ---: | ---: |
| 1 | Portada | Leonardo | 0:30 | 0:30 |
| 2 | El problema: 64,4 MiB frente a 74 KiB | Leonardo | 1:00 | 1:30 |
| 3 | La idea: siete fases | Leonardo | 1:00 | 2:30 |
| 4 | Definición formal: L(S) y W(w) | Leonardo | 1:30 | 4:00 |
| 5 | Fase 1: léxico | Leonardo | 1:00 | 5:00 |
| 6 | Fase 2: sintaxis, XML no es libre de contexto | Leonardo | 1:30 | 6:30 |
| 7 | Fase 3: IR común y clases (a)/(b)/(c) | Leonardo | 1:00 | 7:30 |
| 8 | Fase 4: M = (Q, Σ, Γ, δ, q₀, Z₀, F, w) | Leonardo | 1:30 | 9:00 |
| 9 | Fase 4: el DPDA del árbol recursivo | Leonardo | 1:30 | 10:30 |
| 10 | Fase 4: clase (c) y explosión | Leonardo | 1:30 | 12:00 |
| 11 | Fase 5: emisores y prueba diferencial | Leonardo | 1:00 | 13:00 |
| 12 | Fase 6: streaming, puntero, límites | Leonardo | 1:00 | 14:00 |
| 13 | Bombeo | Bayron | 1:30 | 15:30 |
| 14 | Métricas | Bayron | 1:30 | 17:00 |
| 15 | Seguridad | Leonardo | 1:30 | 18:30 |
| 16 | Aplicación web | Bayron | 1:00 | 19:30 |
| 17 | **Demo en vivo** | Bayron | 5:00 | 24:30 |
| 18 | Conclusiones y preguntas | los dos | 1:00 | 25:30 |

Las notas de cada diapositiva están escritas para estos tiempos. El reparto sigue las pistas: Leonardo, la teoría
y el compilador; Bayron, las métricas, la plataforma y la demo. La seguridad la presenta Leonardo porque hizo la
revisión consolidada (E3).

## 2. Antes de presentar

Media hora antes:

- [ ] La URL pública responde (`GET /api/health`) y la web carga. Poner la URL en la diapositiva 17 (hoy dice
  `[URL PÚBLICA]`).
- [ ] Atlas tiene los 86 casos (los 8 XML del 22 nov se cargan con `cargo run -p atlas`): la pestaña «Casos de
  prueba (Atlas)» los lista.
- [ ] Hacer una compilación de calentamiento (paso 1) para que la primera respuesta no sea la más lenta.
- [ ] Tener abiertos en el portátil, para copiar y pegar:
  - [`tests/schemas/xsd/parse-ok/valid-xml-pedido-01.xsd`](../../tests/schemas/xsd/parse-ok/valid-xml-pedido-01.xsd)
    (el pedido);
  - [`docs/fase5/ejemplos/pedido.json`](../fase5/ejemplos/pedido.json) (payload del árbol);
  - [`tests/schemas/parse-ok/usuarios.json`](../../tests/schemas/parse-ok/usuarios.json) y
    [`ejemplos/usuarios-invalido.json`](ejemplos/usuarios-invalido.json) (comparación).
- [ ] El video de respaldo (§4), descargado en el portátil, no en la nube.
- [ ] Las diapositivas descargadas en PDF por si falla el navegador.

## 3. La demo, paso a paso (Bayron, 5 minutos)

Las pestañas de la web, en el orden en que aparecen: «Editor de esquema», «Autómata», «Árbol de derivación»,
«Comparación DPDA / línea base», «Código generado» y «Casos de prueba (Atlas)». El esquema que usan las demás
pestañas es el que esté en el editor.

1. **Editor de esquema (1 min).** Pegar el XSD del pedido y compilar (Ctrl+Enter). Señalar el resumen: el formato
   que se detectó (XSD, por el «<» inicial), los estados y las transiciones de M (216 y 10 054) y la IR. Si da
   tiempo, romper el XSD (quitar un `</xs:sequence>`) para que se vea el error marcado en el editor con su línea, y
   deshacerlo.
2. **Autómata (30 s).** El grafo de M en Cytoscape, con los estados agrupados por subautómata. Decir que se
   recorta a 400 nodos para que se pueda leer, y que el completo está en la documentación.
3. **Árbol de derivación (30 s).** Pegar `pedido.json` como payload. Es la derivación LL(1) de la gramática de JSON
   de la Fase 2. Pulsar un nodo para que se vean sus bytes y su puntero.
4. **Comparación DPDA / línea base (1 min).** Volver al editor y pegar `usuarios.json`. En la comparación, pegar
   `usuarios-invalido.json`, donde al segundo usuario le falta el `email`. Lo que se espera ver:
   - las dos columnas dicen «inválido» y 1 violación;
   - el DPDA da `required` con el puntero `/users/1` (offset 94);
   - la memoria del DPDA es fija (unos 72 KiB) y la de la línea base depende del payload.
5. **Código generado (1 min).** Descargar el validador en C (ZIP). Validar por WebSocket con un tamaño de bloque
   pequeño (p. ej. 16 bytes) para que se vea avanzar la barra, y señalar que el veredicto es el mismo. En la EC2 la
   validación con el código compilado en el sandbox está apagada a propósito: si preguntan, se explica que exige
   montar el socket de Docker, que equivale a dar root sobre la máquina.
6. **Casos de prueba (Atlas) (1 min).** Ejecutar los 86 casos: 86 de 86. Decir que son las cinco categorías del
   enunciado (válidos, inválidos, malformados, fuera del DPDA puro y extremos) y que cada uno guarda su resultado
   esperado.

Si algo falla en vivo: una frase («la red no responde») y pasar al video. No depurar delante del público.

## 4. Video de respaldo

Grabarlo una vez que la aplicación esté desplegada (F7.7), siguiendo §3 tal cual, sin cortes y sin audio (se
narra en vivo encima). Unos 5 minutos, a 1080p. Guardarlo en el portátil y en una memoria USB.

## 5. Ensayos

Dos ensayos completos con cronómetro, de pie y con las diapositivas en pantalla completa. Anotar aquí el tiempo de
cada parte y lo que se cambia después.

| Ensayo | Fecha | Teoría (1-12) | Resultados (13-15) | App y demo (16-17) | Total | Cambios |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 1 | | | | | | |
| 2 | | | | | | |

## 6. Preguntas probables

**¿Por qué un semianillo tropical y no un autómata booleano?** Porque el enunciado pide reportar la conformidad,
no solo sí o no. Con (ℕ ∪ {∞}, mín, +) el peso del único recorrido es el número de violaciones; ∞ absorbe y separa
lo malformado. El autómata es el mismo: solo cambia lo que se suma (D4).

**¿Es de verdad un DPDA si tiene registros?** Sí: los registros están acotados (máscaras de un número fijo de bits,
contadores y exponentes saturados, lexema de 1 024 bytes), así que el control (q, registros) es finito y la pila es
la única memoria no acotada (automata.md §1).

**¿Por qué no minimizan M entero?** Un DPDA no tiene, en general, un mínimo canónico. Se minimizan las partes
finitas (tries, AFD de `pattern`, espacios léxicos de XSD) con Hopcroft, que es donde están los estados.

**¿Por qué XML no es libre de contexto, si lo reconocen con una pila?** Sobre nombres arbitrarios, emparejar
`<w></w>` no es libre de contexto (bombeo con aᵖbᵖ). Con el esquema, el conjunto de nombres es finito, y entonces sí
lo es (gramatica-xml.md §3-§4).

**¿Por qué `uniqueItems` no va en el autómata?** El lenguaje de los arreglos con elementos distintos no es libre de
contexto: la pila no puede recordar el conjunto de lo visto. Se resuelve con verificación diferida: un hash por
elemento y la comparación al cerrar. Un choque de hash solo puede rechazar de más, y los binarios usan una clave
aleatoria.

**¿Por qué la línea base es más rápida con números?** `serde_json` tiene un bucle de números muy optimizado y
nuestro intérprete hace una consulta a la tabla por byte; usamos tablas densas, sin saltos directos (recorte de
D46). La ventaja que defendemos es la memoria, que no depende de la entrada.

**¿Qué pasa con un payload de varios GB?** Se valida en streaming con la misma memoria: medimos 129,6 MB con
≈ 1,3 MiB de memoria residente en el C. Los offsets son u64; línea y columna se saturan (D9).

**¿Y si el esquema es hostil?** El compilador cuenta todo antes de reservar (presupuesto de 2²⁰ entradas, topes de
productos, de `pattern` y de cierres) y da `limit`. La revisión E3 encontró esquemas de pocos KB que tardaban
minutos; ahora dan `limit` en uno o dos segundos.
