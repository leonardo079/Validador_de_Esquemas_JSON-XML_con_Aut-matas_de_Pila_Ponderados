# Pendientes de la Fase 2 (parser de esquemas)

Observaciones que dejaron abiertas las dos rondas de auditoría del parser LL(1)
de `crates/schema-parser`, ordenadas por prioridad.

## Corregir ahora

### 1. Prueba de rendimiento que depende del reloj

- **Dónde:** `crates/schema-parser/tests/parser.rs`, prueba `muchas_claves_en_tiempo_lineal`.
- **Problema:** exige un tope absoluto de 10 s. Hoy tarda unos 1,2 s en debug, pero
  en una CI muy cargada puede fallar sin que haya un error real.
- **Corrección:** medir el tiempo con N y con 4N claves y comprobar que la razón es
  menor que 8. Si el algoritmo es lineal, la razón ronda 4; si es cuadrático, ronda 16.
  Así la prueba no depende de la velocidad de la máquina.

## Decidir antes del commit

### 2. Claves repetidas dentro de `const` y `enum`

- **Estado actual:** se rechazan en cualquier objeto del esquema, incluidos los valores
  de `const`/`enum`. Está justificado en §4 de `gramatica-esquema.md`.
- **Duda:** la RFC 8259 solo dice que las claves *deberían* ser únicas, no que deban
  serlo. Hay que confirmar si el proyecto quiere esa restricción en los valores
  literales o solo en los objetos-esquema.

### 3. «AST común» con XSD

- **Dónde:** documentación de `crates/schema-parser/src/lib.rs`.
- **Duda:** el AST actual es específico de JSON Schema. Hay que acordar con la
  Persona B si el parser de XSD apuntará a este AST o directamente a la IR, y ajustar
  la frase según lo que se decida. Afecta al diseño de la Fase 3.

### 4. Revisar el manifiesto de errores

- **Dónde:** `tests/schemas/errores.json`.
- **Motivo:** las líneas y columnas esperadas las calculó a mano el desarrollador, y la
  auditoría solo verificó una a mano. Conviene revisar a ojo algunas más.

## Vigilar más adelante

### 5. Margen de pila en debug

- **Estado:** con profundidad 256, el parser usa unos 1,34 MiB de los 2 MiB de pila de
  un hilo en debug (unos 259 KiB en release). La prueba `profundidad_maxima` lo vigila.
- **Cuándo actuar:** si esa prueba empieza a fallar, o si la Fase 3 añade marcos o ramas
  a la recursión. La solución robusta es una pila explícita o `stacker`, que sería una
  dependencia nueva.

### 6. Límite de tamaño del esquema

- **Estado:** cada clave se copia a la tabla de claves vistas, y no hay límite de tamaño
  del esquema ni de claves por objeto.
- **Cuándo actuar:** en la fase de la aplicación web. El límite va en el servidor (tamaño
  máximo de subida), no en el parser.

### 7. Palabras clave fuera del subconjunto

- **Estado:** se aceptan y quedan en el AST como `Kw::Unsupported`.
- **Cuándo actuar:** la Fase 3, al validar contra el meta-esquema, debe rechazarlas.
  Mientras tanto, un esquema con `if`/`then` pasa sin error.
