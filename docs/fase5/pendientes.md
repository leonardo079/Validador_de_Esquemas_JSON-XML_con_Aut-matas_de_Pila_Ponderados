# Fase 5 — pendientes

Lo que dejaron abierto las tareas de la Fase 5. Cada entrada lleva identificador, dónde está, el arreglo propuesto
y la tarea que debe cerrarla. Lo cerrado pasa a `historial.md` (todavía no hay).

## Vigilar más adelante

### Abiertas por la tarea F5.1 (emisor Rust, D74; `docs/fase5/codegen.md`)

#### 1. Revisión de seguridad del validador generado (F5.1-P1; E3)

- **Estado:** F5.1 se hizo sin `/autodev` (decisión del usuario), así que nadie más la revisó, ni
  `code-reviewer` ni `security-auditor`. D35 pide `security-auditor` en el validador generado. El riesgo es bajo:
  el núcleo y el lexer son la copia exacta de los ya revisados en la Fase 4. Lo nuevo es el emisor de literales,
  la fachada, `main` y el escape de cadenas de `escribir_json`.
- **Dónde:** `crates/codegen/src/rust.rs` (plantillas `MAIN` y `VALIDADOR`, `cadena`, en torno a la línea 273).
- **Arreglo propuesto:** incluir el emisor y una salida de ejemplo en la revisión consolidada de las Fases 4-7.
  Puntos a mirar: escape de `rule`; que ningún literal emitido dependa del anfitrión; `Mutex` envenenado
  (`into_inner`); stdin cortado a mitad.
- **Tarea:** E3.

#### 2. Límites fijos en el binario (F5.1-P2; F6.2)

- **Estado:** `STATE_WORDS` y `SV_MAX_DEPTH` salen de `Limits::DEFAULT` al emitir, y el binario no lee opciones.
- **Dónde:** `crates/codegen/src/rust.rs:165` (sustitución de `{{STATE_WORDS}}`) y la plantilla `VALIDADOR`.
- **Arreglo propuesto:** cuando F6.2 exponga `sv_limits_t`, emitir los límites del caso (campo `limits` del
  corpus) o leerlos en `main`. La memoria tendría que dimensionarse con esos límites: un `static` del tamaño
  máximo permitido, o `Validador::new` con memoria propia.
- **Tarea:** F6.2.

#### 3. `main` solo en conformidad total (F5.1-P3; F7.5)

- **Estado:** la fachada admite `Mode::EarlyFail`, pero `main` siempre usa `Mode::Total`
  (`crates/codegen/src/rust.rs:125`).
- **Arreglo propuesto:** si F7.5 quiere el fallo temprano en el sandbox, `sandbox-run` no pasa argumentos al
  binario. Habría que acordar con Bayron una forma fija: un segundo binario, o una primera línea en stdin. Si no
  hace falta, se deja así.
- **Tarea:** F7.5 (decisión de Bayron).

#### 4. Puntero JSON del primer error (F5.1-P4; F6.1)

- **Estado:** la línea JSON no tiene `pointer` (el `expected.pointer` del corpus), porque el núcleo aún no lo
  calcula.
- **Dónde:** `escribir_json` en la plantilla `VALIDADOR` (`crates/codegen/src/rust.rs:273`).
- **Arreglo propuesto:** cuando F6.1 añada el puntero al `Report` de `runtime-rs`, la copia lo trae sola. Falta
  añadir el campo `pointer` en `escribir_json` y en `json_esperado` de `crates/codegen/tests/rust.rs`, y el mismo
  campo en el emisor C (F5.2).
- **Tarea:** F6.1.

#### 5. Lo que F7.5 tiene que saber (F5.1-P5; cierra F7.3-P2)

- **Estado:** F7.3-P2 (`docs/fase7/pendientes.md` §14) pedía código generado que compilara sin dependencias.
  `codegen::rust::emit` ya da archivos planos que pasan `server::sandbox::comprobar` y compilan con la orden fija
  de la imagen (`rustc --edition 2021 -O main.rs`). La prueba `main_real_en_todo_el_corpus` lo hace con todo el
  corpus.
- **Contrato** (`docs/fase5/codegen.md` §3 y §6):
  - stdout es una línea JSON con `valid`, `violations` (`null` si es ∞), `errorKind`, `offset`, `line`, `column`,
    `rule` y `keyword` (los campos del DPDA de `/api/compare` más `keyword`);
  - códigos 0 válido, 1 inválido, 2 malformado o límite, 3 error interno o de E/S;
  - unos 274 KB de fuentes por esquema.
- **Arreglo propuesto:** en F7.5, pasar `emit(&a)` como `Trabajo::archivos` y marcar F7.3-P2 como cerrado en
  `docs/fase7/pendientes.md` (archivo de la pista B). La caché del binario por esquema (F7.3-P1) sigue abierta.
- **Tarea:** F7.5 (Bayron).

#### 6. Memoria del validador con marcos muy grandes (F5.1-P6; Info, informe E4)

- **Estado:** `STATE_WORDS` = zona + 256 × (marco máximo + 2) + 8 192. `main` la declara como
  `static Mutex<[u64; STATE_WORDS]>` (`crates/codegen/src/rust.rs:115`): va en `.bss` y no cuesta en el binario,
  pero `rustc` la evalúa al compilar. Con un marco de miles de palabras serían decenas de MiB. Ningún esquema del
  corpus pasa de 74 KiB.
- **Arreglo propuesto:** dar la cifra en el informe («tamaño de `sv_state_t`») junto a `maxFrameBytes` de
  `schemac automaton`. Si algún esquema real la hace crecer, poner un tope en `automaton` (`BuildError::Limit`
  con el tamaño), igual que F4-U6.
- **Tarea:** E4 (y F6.2 si se decide el tope).

#### 7. Copia entera de los dos crates (F5.1-P7; Info)

- **Estado:** se copian también `dot.rs` y `short.rs` del lexer y el validador del otro formato, aunque no se
  usen, y `main.rs` lleva `#![allow(dead_code, unused_imports)]` para que no salgan avisos
  (`crates/codegen/src/fuentes.rs:68`, `crates/codegen/src/rust.rs:94`).
- **Arreglo propuesto:** ninguno por ahora: copiar sin elegir mantiene la copia idéntica y la vigila una sola
  prueba. Si F5.2 necesita un núcleo más pequeño en C, se decide allí.
- **Tarea:** F5.2 (decidir) o nada.

#### 8. Criterios que hereda F5.2 (emisor C)

- La misma línea JSON y los mismos códigos de salida que en Rust (§5), para que F5.3 compare las salidas tal
  cual.
- El mismo recorrido de las tablas (orden de `Tables`) y la desestructuración exhaustiva en el emisor.
- La prueba diferencial F5.3 puede reutilizar `grupos()` y `json_esperado` de `crates/codegen/tests/rust.rs`.
