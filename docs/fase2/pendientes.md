# Pendientes de la Fase 2 (parser de esquemas)

Observaciones que dejaron abiertas las dos rondas de auditoría del parser LL(1)
de `crates/schema-parser`, ordenadas por prioridad.

## Corregir ahora

### 1. Prueba de rendimiento que depende del reloj — RESUELTO (mitigado)

- **Resuelto (mitigado, no garantizado):** la prueba compara t(4N)/t(N) < 10 con N = 10⁴ en `properties`, `const`, `$defs` y `dependentRequired`, con lotes calibrados de ≥ ~50 ms por muestra y el mínimo de 3 muestras intercaladas, y repite la estimación hasta 3 veces: falla solo si las tres superan el umbral. Con 4× de sobresuscripción (32 `yes` en 8 núcleos): 0/30 fallos en release y 0/10 en debug, aunque las razones sueltas van de 1,1 a 15,9. La búsqueda cuadrática reintroducida sigue fallando (26,4–27,2 en los tres intentos, release). Con carga mayor aún puede fallar sin regresión, con baja probabilidad.
- **Dónde:** `crates/schema-parser/tests/parser.rs`, prueba `muchas_claves_en_tiempo_lineal`.
- **Problema (original):** exigía un tope absoluto de 10 s; en una CI muy cargada
  podía fallar sin que hubiera un error real.
- **Corrección propuesta originalmente:** medir el tiempo con N y con 4N claves y
  comprobar que la razón es menor que 8. La segunda auditoría mostró que una sola
  estimación no aguanta carga alta; de ahí los lotes, el umbral 10 y los reintentos.

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
