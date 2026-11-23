# Propuesta: DTD como tercer lenguaje de esquema

> **Estado: propuesta, sin aplicar.** Toca `docs/00-alcance.md` y la detección de formato de D22 (CLI, servidor y
> web), que son interfaces compartidas: necesita el visto bueno de las dos personas (D42) antes de que entre una
> línea de código. Número de decisión provisional **D86**: antes de registrarla, `git pull` y tomar el siguiente
> libre (COLABORAR.md).
>
> Redactada el 22 nov 2026 en la rama `DTD`.

## 1. Qué se propone, en una frase

Aceptar un **subconjunto de DTD (XML 1.0 §3) como archivo de esquema aparte**, con un frontal nuevo
`crates/dtd` que baja a la **misma IR** que JSON Schema y XSD. A partir de la Fase 4 (autómata, código C/Rust,
generador de payloads, servidor, web) no cambia nada.

## 2. Situación actual y por qué no contradice D8

- Hoy el proyecto **no** tiene DTD como lenguaje de esquema. Solo acepta JSON Schema 2020-12 y XSD
  (`docs/00-alcance.md` §2-§3).
- D8 prohíbe `<!DOCTYPE` **dentro del documento XML**, con `DoctypeForbidden` en el lexer
  (`crates/lexer/src/xml.rs:491`). Así se mitigan XXE y *billion laughs*, como pide el enunciado.
- La propuesta **mantiene D8 tal cual para los payloads y para los XSD**. La DTD llega como esquema
  independiente, igual que hoy un `.xsd`, nunca dentro del payload. Un payload con `<!DOCTYPE` sigue siendo
  `Malformed`.
- Dentro de la DTD tampoco se admiten entidades (`<!ENTITY`, `%pe;`) ni identificadores externos, así que la
  mitigación no pierde nada (§4.3).

**El enunciado no exige DTD.** Pide JSON Schema «con extensión a un subconjunto de XSD». Esto es un añadido
opcional, y su valor es sobre todo de demo e informe: un tercer frontal que reutiliza la IR, y un lenguaje de
esquema cuyos modelos de contenido son expresiones regulares deterministas (XML 1.0, apéndice E) que pasan
directo a Glushkov.

## 3. Por qué encaja sin tocar la IR

| Pieza de la IR (`docs/fase3/ir.md` §5.2, `docs/fase3/xsd.md` §4) | En DTD |
| --- | --- |
| Nodo 0, el documento: `Choice` de una partícula por elemento global | Toda `<!ELEMENT>` es global. Sin `DOCTYPE` en el payload no hay raíz declarada, así que la raíz puede ser **cualquier elemento declarado**, como con los globales de XSD. |
| `Element` con `Content` (`Seq`/`Choice`/`Occurs`) | Modelo de hijos: `,` `\|` `?` `*` `+` (sin contadores: todo es categoría (a)). |
| `Element` con contenido vacío | `EMPTY` |
| `SimpleContent` → `Text` de `builtin/string` | `(#PCDATA)` |
| `Attributes` con máscara de requeridos | `<!ATTLIST>` con `#REQUIRED` / `#IMPLIED` |
| `Text` con enumeración | Atributo enumerado `(a\|b\|c)` |
| `fixed` / `default` de atributo | `#FIXED "v"` / valor por defecto |
| `Format::Xml { target_namespace: None }` | DTD no conoce espacios de nombres |

Ningún campo nuevo en la IR, en `VerifiedIr` ni en el JSON v2 de `schemac ir` (D17). Tampoco cambian la
API C-ABI ni la API del servidor: el servidor ya informa `"xml"` para XSD y lo mismo serviría para DTD
(`server/src/compilar.rs:38`).

## 4. Subconjunto propuesto (para la nueva §3bis de `00-alcance.md`)

### 4.1 Soportado

| Construcción | Soporte | Cat. |
| --- | --- | --- |
| `<!ELEMENT n EMPTY>` | Sin hijos ni texto. | (a) |
| `<!ELEMENT n (#PCDATA)>` | Solo texto (`xs:string`). | (a) |
| `<!ELEMENT n (modelo)>` | Hijos con `,` `\|` y los sufijos `?` `*` `+`, anidables. Determinista como exige XML 1.0 (apéndice E); si no lo es, se rechaza con la posición de las dos partículas en conflicto. | (a) |
| `<!ATTLIST n a CDATA …>` | Texto libre. | (a) |
| `<!ATTLIST n a (v1\|v2\|…) …>` | Enumeración. | (a) |
| `#REQUIRED` / `#IMPLIED` | Requerido (máscara de bits) / opcional. | (b) |
| `"valor"` / `#FIXED "valor"` | Valor por defecto (el validador no lo inserta) / valor fijo. El valor se comprueba contra la enumeración al compilar. | (a) |
| Varias `<!ATTLIST>` del mismo elemento | Se unen. | — |
| Comentarios, instrucciones de procesamiento y una declaración de texto `<?xml … encoding="UTF-8"?>` al inicio | Se aceptan y se ignoran. | — |

### 4.2 Fuera de alcance (error con posición y motivo, D6)

- `ANY` (equivale a `xs:any` con contenido mixto, ya excluido en XSD).
- Contenido mixto `(#PCDATA|a|b)*` (`00-alcance.md` §1.2: no hay contenido mixto).
- Atributos `ID`, `IDREF`, `IDREFS`: unicidad y referencias globales, como `xs:key`/`xs:keyref`. No caben en un
  DPDA con registros acotados.
- Atributos `ENTITY`, `ENTITIES`, `NOTATION` y `<!NOTATION>`.
- `NMTOKEN` / `NMTOKENS`, en la primera versión. Se podrían añadir después como una `pattern` → AFD (c) con la
  clase de `NameChar` restringida a ASCII, documentada como desviación.
- Nombres con `:`.

### 4.3 Prohibido por seguridad (se mantiene el espíritu de D8)

- `<!ENTITY …>`, tanto generales como de parámetro, y cualquier referencia `%nombre;` o `&nombre;` que no sea
  de las 5 predefinidas o de carácter.
- Identificadores externos (`SYSTEM`, `PUBLIC`) y secciones condicionales `<![INCLUDE[` / `<![IGNORE[`.
- `<!DOCTYPE` dentro del archivo DTD: una DTD externa no lo lleva, y aceptarlo abriría la puerta al subconjunto
  interno.

Nada abre archivos ni red y nada se expande: el tamaño de la IR sigue siendo lineal en el tamaño del esquema.

### 4.4 Desviaciones de XML 1.0 (todas rechazan de más, ninguna acepta de más)

1. Un nombre usado en un modelo de contenido o en una `<!ATTLIST>` sin su `<!ELEMENT>` es error `unresolved`
   (en XML es solo advertencia).
2. Un atributo declarado dos veces para el mismo elemento es error (en XML gana la primera declaración).
3. Los valores de un atributo enumerado se comparan **sin** la normalización de XML (§3.3.3: quitar y colapsar
   espacios), porque la faceta `whiteSpace` está fuera de alcance en la IR. `a=" x "` se rechaza aunque XML lo
   aceptaría.

## 5. Cambios en interfaces compartidas (lo que se aprueba)

1. **`docs/00-alcance.md`:** título y párrafo inicial («JSON Schema, XSD y DTD») y una nueva sección con §4.
   §1.2 añade una aclaración: la prohibición de `<!DOCTYPE` se refiere al payload, y la DTD se da aparte como
   esquema.
2. **D22 (detección por contenido)**, que modifican tres sitios que hoy hacen lo mismo (`crates/cli/src/main.rs:148`,
   `server/src/compilar.rs:48`, `web/src/api.ts:195`). Regla nueva: tras el BOM y los espacios, se saltan una
   `<?xml …?>` inicial y los comentarios `<!-- … -->`.
   - Si lo siguiente empieza por `<!`, es **DTD**.
   - Si es otro `<`, es XSD.
   - Si no, JSON.

   El coste es O(prólogo), sin recursión. Se propone poner la función en un solo sitio (por ejemplo,
   `ir::formato` o un módulo nuevo que usen CLI y servidor) para que los tres no se desincronicen.
   *Alternativa:* una opción `--format dtd` o un campo `format` en `POST /api/schemas`. Se descarta porque
   cambia la API del servidor (D82/D84) y D22 se tomó justo para no depender del nombre del archivo.
3. **Web:** el selector de archivos acepta `.dtd`, aparece «Formato detectado: DTD» y se añade un ejemplo DTD
   en `web/src/ejemplos.ts`. No cambia ningún contrato.

No cambian D8, D17, la IR, la API C-ABI ni las rutas del servidor.

## 6. Diseño del frontal (`crates/dtd`)

Sigue el patrón de `crates/xsd` y de D21: depende solo de `ir` (y de `lexer` solo para `Pos`/`Span`). No depende
de `xsd`, `semantic` ni `schema-parser` (D13).

| Fase del curso | Pieza | Archivo de teoría |
| --- | --- | --- |
| 1. Léxico | AFD del tokenizador DTD con tabla de transición y `.dot` generado por `make docs`. Tokens: `<!ELEMENT`, `<!ATTLIST`, `Name`, `#PCDATA`, `#REQUIRED`, `#IMPLIED`, `#FIXED`, `EMPTY`, `ANY`, `CDATA`, los tipos tokenizados, `(` `)` `\|` `,` `?` `*` `+` `>`, literal entre comillas, comentario e instrucción de procesamiento. `<!ENTITY`, `%`, `<![` y `<!DOCTYPE` son errores léxicos propios, como `DoctypeForbidden`. | `docs/fase1/lexico-dtd.md` |
| 2. Sintaxis | Gramática LL(1) `docs/fase2/dtd.ll1`, comprobada con `scripts/ll1.py` (`make docs-check`); parser descendente con la profundidad de paréntesis acotada (`MAX_DEPTH_LIMIT`, D7); AST propio (D3). | `docs/fase2/gramatica-dtd.md`, `ll1-dtd.md` (generado) |
| 3. Semántica | Tabla de símbolos de elementos; declaraciones repetidas; `unresolved`; determinismo del modelo (Glushkov sin contadores, propio de `dtd` para no depender de `xsd::upa`); valores de `#FIXED` y por defecto contra la enumeración; bajada a la IR seguida de `verify` (D15). Todo iterativo. | `docs/fase3/dtd.md` |
| 4-7 | Nada nuevo: autómata, código, generador de payloads (`bench::generar_xml` trabaja sobre `VerifiedIr`), servidor y web reciben la misma IR. | — |

## 7. Tareas propuestas (para `tasks/CHECKLIST.md`, si se aprueba)

| ID | Tarea | Depende | Revisión (D35) |
| --- | --- | --- | --- |
| **DTD.0** | Registrar D86 y la nueva sección de `00-alcance.md` (§4-§5 de este documento) con el visto bueno de los dos. | — | — |
| **DTD.1** | Lexer DTD (AFD, tabla, `.dot`, pruebas por bloques de 1/7/4096 y oráculo) y gramática LL(1). | DTD.0 | code-reviewer |
| **DTD.2** | Parser LL(1), AST y rechazo de lo que está fuera del subconjunto con categoría y motivo. | DTD.1 | code-reviewer |
| **DTD.3** | Análisis, determinismo y bajada a la IR verificada; doradas de IR para 2-3 esquemas. | DTD.2 | code-reviewer |
| **DTD.4** | Detección de formato común (D22 enmendada) en `schemac`, servidor y web; ejemplo DTD en la web. | DTD.3 | code-reviewer + security-auditor (el servidor recibe el esquema sin confiar en él) |
| **DTD.5** | Casos en `tests/cases` (válidos, inválidos, malformados, `<!ENTITY` y `SYSTEM` rechazados), fila en `docs/fase6/mitigaciones.md`, objetivo de fuzzing `esquema_dtd`, carga en Atlas y prueba diferencial contra la misma IR escrita en XSD. | DTD.4 | security-auditor |
| **DTD.6** | Documentación final: `00-alcance.md`, `docs/README.md`, `informe-final.md` y `arquitectura.md`. | DTD.5 | — |

Para DTD.5, una prueba diferencial que cuesta poco: escribir el mismo esquema en DTD y en XSD y comprobar que
la IR exportada (JSON v2) coincide, salvo los orígenes. Eso demuestra la bajada sin otra línea base. La línea
base con libxml2 (D54) podría además validar con DTD, pero eso exige quitar su capa de política D8 solo para ese
caso. Se deja fuera.

## 8. Riesgos y cuándo hacerlo

- **Calendario:** la entrega y la demo son **hoy, 22 nov**. E6 («congelar código») estaba previsto para el
  19 nov. Meter un frontal nuevo hoy pone en riesgo un sistema desplegado y probado (F7.7, D85). **Recomendación:
  no hacerlo antes de la demo.** Si se quiere mencionar en la presentación, basta una diapositiva de «trabajo
  futuro» con §3 de este documento: muestra que la IR común admite un tercer frontal sin tocar las Fases 4-7.
- **Superficie de ataque:** el servidor compilaría DTD no confiables. Se mitiga con §4.3 (sin entidades ni
  referencias externas), los límites de D82 (1 MiB por petición) y de `schemac` (16 MiB), el parser iterativo
  (D7) y el objetivo de fuzzing de DTD.5.
- **Tres copias de D22:** si se cambia una y no las otras, la web diría «XSD» para una DTD. Por eso §5.2 propone
  una sola función.

## 9. Preguntas para el visto bueno

1. ¿Se hace? Y si se hace, ¿después de la entrega o como opcional junto a E8 (WASM)?
2. ¿Detección por contenido (§5.2, recomendada) o un parámetro explícito de formato?
3. ¿Raíz = cualquier elemento declarado (recomendado, como XSD) o una opción `--root`? La opción cambiaría la
   CLI y la API.
4. ¿`NMTOKEN`/`NMTOKENS` fuera de alcance en la primera versión (recomendado) o como `pattern` ASCII?
5. ¿Pista y responsable? El frontal es de la pista A (compilador) y DTD.4 toca la pista B (servidor y web).
