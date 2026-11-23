# Propuesta: DTD como tercer lenguaje de esquema

> **Estado: aprobada (D86, 22 nov).** Visto bueno de Bayron el 22 nov, con las recomendaciones de §9: detección por
> contenido en una sola función, raíz = cualquier elemento declarado y `NMTOKEN`/`NMTOKENS` fuera de la primera
> versión. Alcance registrado en `docs/00-alcance.md` §3bis; tareas DTD.0-DTD.6 en `tasks/CHECKLIST.md`, todas de
> Bayron, en la rama `DTD` (`main` no se toca hasta cerrar).
>
> Redactada el 22 nov 2026 en la rama `DTD`. Abajo, el texto original de la propuesta.
>
> **Enmiendas a D86 pendientes del visto bueno de Bayron:** §10, de la revisión de Leonardo del commit `27b7a2b`
> (DTD.0). Tocan `docs/00-alcance.md` §3bis y D86, que son interfaces compartidas, así que solo están propuestas:
> no se ha cambiado ni el alcance ni el plan. Bloquean DTD.1 los puntos E1 y E4 (el lexer y la detección dependen de
> ellos).

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

> **Sustituido por la enmienda E3 de §10:** comparar el JSON v2 no sirve. La comparación pasa a hacerse por
> veredictos.

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

## 10. Enmiendas a D86 (revisión de Leonardo, 22 nov; pendientes del visto bueno de Bayron)

Revisión del commit `27b7a2b` (DTD.0). D86 y §3bis siguen bien la propuesta, pero hay cinco puntos que conviene
cambiar antes de DTD.1. Para cada uno van el problema y el texto que se propone. Cuando Bayron los apruebe, se
aplican en `docs/00-alcance.md` §3bis, en la fila de D86 de `tasks/plan-general.md` y en sus líneas de
`tasks/CHECKLIST.md`. Leonardo no edita esas líneas porque son de Bayron.

### E1. La detección no debe mandar a la DTD un XSD con `<!DOCTYPE` (bloquea DTD.1)

**Problema.** Con la regla de D86 («tras BOM, espacios, una `<?xml …?>` y comentarios, `<!` es DTD»),
`tests/schemas/xsd/parse-error/doctype.xsd` (`<?xml version="1.0"?>` + `<!DOCTYPE x>` + `<xs:schema>`) deja de
llegar al frontal XSD. Rompe `crates/cli/tests/check.rs:122-126`, que espera el error léxico de XSD en la
línea 2, columna 3. La seguridad no se pierde, porque el frontal DTD también lo rechaza, pero cambian el mensaje y la
prueba.

**Texto propuesto** (final del primer párrafo de §3bis y regla (2) de D86):

> El formato se detecta por contenido (D22 enmendada por D86). Tras el BOM, los espacios, una `<?xml …?>` inicial
> y los comentarios `<!-- … -->`:
> - `<!DOCTYPE` (en cualquier caja) es **XSD**, para que el frontal XSD lo rechace como hoy (D8).
> - Cualquier otro `<!` es **DTD**.
> - Otro `<` es XSD.
> - Lo demás, o nada, es JSON.
>
> Si la `<?xml` o un comentario no se cierran, es XSD y el error lo da el lexer XML en su posición. El coste es
> O(prólogo), sin recursión ni reservas.

**Tabla de casos** (para la prueba de E2):

| Entrada | Formato |
| --- | --- |
| `<!ELEMENT a EMPTY>` | DTD |
| `<?xml version="1.0"?>` + `<!-- c -->` + `<!ATTLIST a b CDATA #IMPLIED>` | DTD |
| `<!ENTITY x "y">` y `<![INCLUDE[` | DTD (y el frontal DTD lo rechaza) |
| `<?xml version="1.0"?>` + `<!DOCTYPE x>` + `<xs:schema/>` | XSD |
| `<!doctype x><xs:schema/>` | XSD |
| `<!-- c --><xs:schema/>` | XSD |
| `<!-- sin cerrar` | XSD |
| BOM + espacios + `{}` | JSON |
| vacío | JSON |

### E2. Dónde vive la detección, y la web

**Problema.** D86 dice «una sola función que usan CLI, servidor y web». La web es TypeScript
(`formatoProbable`, `web/src/api.ts:195`) y no puede llamar a Rust. Tampoco puede leer el formato del servidor:
`format` vale `"xml"` para XSD y para DTD (`server/src/compilar.rs:36-40`), y añadir `"dtd"` cambiaría la API
(D82).

**Texto propuesto** (regla (2) de D86):

> Una sola función en Rust, `dtd::formato(&[u8]) -> Formato { Json, Xsd, Dtd }` en `crates/dtd`, que usan
> `schemac` (sustituye a `formato` de `crates/cli/src/main.rs:148`) y el servidor (sustituye a la «copia literal»
> de `server/src/compilar.rs:48`).
> - Va en `dtd` porque el CLI y el servidor ya dependen de ese crate. No va en `ir`, que es interfaz compartida,
>   ni en `lexer`, que se incrusta en el validador generado.
> - La web mantiene su copia en TypeScript (`formatoProbable` pasa a devolver `"json" | "xsd" | "dtd"`, solo para la
>   etiqueta «Formato detectado»).
> - Las dos se prueban con la **misma tabla de casos**, `tests/schemas/formato.json` (la de E1), con `cargo test`
>   y `vitest`.
> - El campo `format` de la API sigue siendo el de la IR (`"xml"` para XSD y DTD): no cambia la API.

### E3. La diferencial de DTD.5 se hace por veredictos, no comparando el JSON v2

**Problema** (error de la propuesta original, §7). La bajada de XSD crea nodos sintéticos
(`#/simpleContent/xs:string`, `#/builtin/string`), y el orden de los `NodeId` depende de los tipos con nombre y
anónimos (`docs/fase3/xsd.md` §4). Una DTD equivalente no produce el mismo JSON v2, ni siquiera quitando los
orígenes.

**Texto propuesto** (línea DTD.5 del checklist; la frase de D86 no cambia):

> … diferencial DTD frente a XSD **por veredictos**. Para 2-3 pares (DTD, XSD) equivalentes, se generan payloads
> válidos (`bench::generar_xml_valido`), inválidos (`generar_xml_invalido`) y malformados (`malformar`) desde las
> dos IR, y los dos autómatas tienen que dar el mismo veredicto y la misma clase de error (`clase_dpda`).

### E4. `SYSTEM` y `PUBLIC` no son palabras prohibidas (bloquea DTD.1)

**Problema.** §3bis.3 dice que los identificadores externos (`SYSTEM`, `PUBLIC`) «son errores léxicos». Esas
palabras solo aparecen dentro de `<!ENTITY`, `<!DOCTYPE` y `<!NOTATION`, que ya se rechazan en su `<!`. Si el lexer
las tratara como error, rechazaría `<!ELEMENT SYSTEM EMPTY>`, que es XML válido. Lo mismo pasaría con un `%` dentro
de un valor por defecto entre comillas: en un `AttValue`, XML no reconoce referencias de parámetro, así que ahí
`%` es un carácter más.

**Texto propuesto** (§3bis.3):

> Son errores léxicos, en el `<!` de la declaración o en el `%`/`&` de la referencia:
> - `<!ENTITY …>`, tanto generales como de parámetro;
> - `<!DOCTYPE` dentro del archivo DTD;
> - las secciones condicionales `<![INCLUDE[`/`<![IGNORE[`;
> - una referencia de parámetro `%nombre;` fuera de un literal;
> - dentro de un literal, `&nombre;` que no sea una de las cinco predefinidas ni una referencia de carácter.
>
> Los identificadores externos (`SYSTEM`, `PUBLIC`) solo pueden ir dentro de esas declaraciones (o de
> `<!NOTATION`, §3bis.2), así que quedan excluidos sin prohibir las palabras: `SYSTEM` y `PUBLIC` son nombres válidos
> de elemento y de atributo. Nada abre archivos ni red y nada se expande: la IR sigue siendo lineal en el tamaño
> del esquema.

### E5. Solo se despliega desde `main`

**Problema.** D86 protege `main` («no se toca hasta cerrar»), pero el despliegue no mira la rama.
`infra/desplegar.ps1` construye las imágenes con el árbol **local** y en la EC2 hace `git pull --ff-only` sobre la
rama que tenga (línea 60). Además, en el remoto `origin` la rama por defecto es `DTD` (`origin/HEAD -> origin/DTD`).
Si alguien despliega desde la rama `DTD`, el trabajo a medias llega a la URL pública de la demo.

**Texto propuesto** (añadir a D86):

> Mientras la rama `DTD` esté abierta, solo se despliega desde `main`: la copia local y la de la EC2, en `main`
> antes de ejecutar `infra/desplegar.ps1`.

Mejor aún: que el script lo compruebe. Si `git rev-parse --abbrev-ref HEAD` no es `main`, el script termina. Es un
cambio de la pista B y lo decide Bayron.

### Registro (no bloquea)

- DTD.1-DTD.3 dicen «Pista: A», pero las lleva Bayron. El acuerdo S1 del 13 nov es pista A = Leonardo y pista
  B = Bayron, y el «Retomar aquí» de DTD.0 solo se actualizó en la línea de la pista B. Propuesta: «Pista: B
  (excepción, D86)» en DTD.1-DTD.6, para que «Retomar aquí» y el checklist cuenten lo mismo.
- En la copia de Leonardo, `origin/DTD` seguía en `114c40d` al revisar: falta `git push` de `07c806a` y `27b7a2b`
  (o un `fetch`).
