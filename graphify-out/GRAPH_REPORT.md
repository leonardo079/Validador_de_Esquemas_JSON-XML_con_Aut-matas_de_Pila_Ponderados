# Graph Report - Validador_de_Esquemas_JSON-XML_con_Aut-matas_de_Pila_Ponderados  (2026-10-20)

## Corpus Check
- 177 files · ~79,034 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 22 file(s) not represented in the graph (top: (none) 10, .ll1 4, .proptest-regressions 3)

## Summary
- 1225 nodes · 2207 edges · 60 communities (45 shown, 15 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 63 edges (avg confidence: 0.85)
- Token cost: 315,159 input · 0 output

## Community Hubs (Navigation)
- XML Lexer Actions
- JSON Value AST & Classify
- JSON Lexer Actions
- Lexer Errors & Positions
- Test Corpus Harness
- UTF-8 Tests & Cycle Graph
- Schema Keyword Enum
- Schema Parser Core
- XML Lexer Tests
- JSON Token Events
- CLI schemac Check
- Symbol Table & Pointers
- JSON Lexer Tests
- Parser Tests
- UTF-8 DFA Table
- Exact Number Lexemes
- Issue Creation Script
- Cycle Detection Tests
- XML Tokenizer DFA Diagram
- Phase 1 Lexical Design
- Project Architecture & Crates
- Keyword Value Classes
- Unsupported Keyword Rejection
- JSON Tokenizer DFA Diagram
- Ref Resolution Problems
- Phase 2 Pending Items
- DFA Transition Tables
- UTF-8 Decoder Steps
- Scope & Supported Subset
- JSON CFG & LL(1)
- Semantic Error Kinds
- Weighted DPDA & Test Theory
- Schema Metalanguage Grammar
- Phase 3 Symbols & Refs
- Repo Templates & NoSQL Corpus
- CI & Streaming Invariants
- Meta-Schema Validation
- Non-Productive Cycles Theory
- DOT Diagram Export
- Byte Class Partition Tests
- Keyword Categories
- XML Grammar & Non-CF Proof
- Workspace Crates
- UTF-8 DFA Diagram
- Prettier Config
- Draft-7 Schema Fixture
- DOT Writer (a)
- DOT Writer (b)
- DOT Writer (c)
- Issue Config
- Error Issue Template
- runtime-rs Crate

## God Nodes (most connected - your core abstractions)
1. `Kw` - 66 edges
2. `Value` - 53 edges
3. `SemanticError` - 34 edges
4. `Pos` - 31 edges
5. `LexErrorKind` - 30 edges
6. `Span` - 29 edges
7. `Schema` - 27 edges
8. `Parser<'a>` - 24 edges
9. `Str` - 22 edges
10. `Tok` - 22 edges

## Surprising Connections (you probably didn't know these)
- `Tarjan iterativo (SCC) con CSR` --references--> `check_cycles()`  [INFERRED]
  docs/fase3/ciclos.md → crates/semantic/src/cycles.rs
- `Meta-esquema 2020-12 restringido M'` --references--> `check_meta_schema()`  [EXTRACTED]
  docs/fase3/meta-esquema.md → crates/semantic/src/meta.rs
- `Clasificacion application (SameInstance/ChildInstance/None)` --references--> `Application`  [EXTRACTED]
  docs/fase3/ciclos.md → crates/semantic/src/cycles.rs
- `Tabla de simbolos T(S)=(N,id,padre,A,D,rho)` --references--> `SymbolTable`  [EXTRACTED]
  docs/fase3/referencias.md → crates/semantic/src/symbols.rs
- `Fase 1 - Lexical analysis` --references--> `crate lexer (Phase 1)`  [INFERRED]
  docs/fase1/lexico.md → README.md

## Import Cycles
- 1-file cycle: `crates/lexer/src/lib.rs -> crates/lexer/src/lib.rs`

## Hyperedges (group relationships)
- **Compiler pipeline phase crates** — readme_crate_lexer, readme_crate_schema_parser, readme_crate_semantic, readme_crate_automaton, readme_crate_codegen [EXTRACTED 1.00]
- **Category (c) fallback strategies** — docs_00_alcance_combinators, docs_00_alcance_bounded_determinization, docs_00_alcance_deferred_verification, docs_00_alcance_pattern_dfa [EXTRACTED 1.00]
- **Phase 1 lexer DFAs** — docs_fase1_lexico_utf8_dfa, docs_fase1_lexico_json_tokenizer_dfa, docs_fase1_lexico_xml_tokenizer_dfa, docs_fase1_lexico_byte_classes [EXTRACTED 1.00]
- **Pipeline semantico Fase 3 (meta-esquema -> tabla de simbolos -> ciclos)** — docs_fase3_meta_esquema_meta_esquema_restringido, docs_fase3_referencias_symbol_table, docs_fase3_ciclos_ciclo_no_productivo, docs_fase3_meta_esquema, docs_fase3_referencias, docs_fase3_ciclos [EXTRACTED 1.00]
- **Gramaticas LL(1) de JSON, esquema y XML generadas por ll1.py** — docs_fase2_gramatica_json_json_ll1_grammar, docs_fase2_gramatica_esquema_schema_ll1_grammar, docs_fase2_gramatica_xml_xml_ll1_grammar, docs_fase2_ll1_esquema_ll1_script, docs_fase2_gramatica_json_condiciones_ll1 [INFERRED 0.85]
- **Seguridad de pila D7 (limite 256, recorridos iterativos, Tarjan iterativo)** — docs_fase2_gramatica_esquema_max_depth_limit, docs_fase2_pendientes_margen_de_pila_d7, docs_fase3_meta_esquema_recorrido_iterativo, docs_fase3_ciclos_tarjan_iterativo [INFERRED 0.85]
- **JSON token classes recognized from state s0** — docs_fase1_afd_json_value_start_state, docs_fase1_afd_json_structural_whitespace, docs_fase1_afd_json_literal_keywords, docs_fase1_afd_json_number_subautomaton, docs_fase1_afd_json_string_subautomaton [EXTRACTED 1.00]
- **Lexer error paths (implicit error, LoneSurrogate, InvalidUtf8)** — docs_fase1_afd_json_error_state, docs_fase1_afd_json_lonesurrogate_error, docs_fase1_afd_json_invalidutf8_error, docs_fase1_afd_json_afd_utf8 [EXTRACTED 1.00]
- **UTF-8 DFA state structure** — docs_fase1_afd_utf8_s0_inicio, docs_fase1_afd_utf8_continuation_states, docs_fase1_afd_utf8_restricted_lead_states, docs_fase1_afd_utf8_error_state [EXTRACTED 1.00]
- **Despacho desde '<' hacia etiqueta, cierre, comentario/CDATA e instruccion de procesamiento** — docs_fase1_afd_xml_start_tag, docs_fase1_afd_xml_end_tag, docs_fase1_afd_xml_comment, docs_fase1_afd_xml_cdata, docs_fase1_afd_xml_processing_instruction [EXTRACTED 1.00]
- **Referencias en valores de atributo con regreso por registro** — docs_fase1_afd_xml_attribute, docs_fase1_afd_xml_entity_reference, docs_fase1_afd_xml_char_reference, docs_fase1_afd_xml_return_register [EXTRACTED 1.00]
- **Errores dependientes de registro no dibujados** — docs_fase1_afd_xml_error_state, docs_fase1_afd_xml_undefined_entity, docs_fase1_afd_xml_invalid_char_ref, docs_fase1_afd_xml_invalid_xml_char, docs_fase1_afd_xml_invalid_utf8 [EXTRACTED 1.00]

## Communities (60 total, 15 thin omitted)

### Community 0 - "XML Lexer Actions"
Cohesion: 0.02
Nodes (107): A_ATTR_BEGIN, A_CD_FLUSH1, A_CD_FLUSH2, A_CD_RB, A_CHAR, A_EMPTY_CLOSE, A_ERR, A_ETAG_CLOSE (+99 more)

### Community 1 - "JSON Value AST & Classify"
Cohesion: 0.05
Nodes (68): Sink, Span, Json, JsonKind, Array, Bool, Null, Number (+60 more)

### Community 2 - "JSON Lexer Actions"
Cohesion: 0.02
Nodes (89): A_ERR, A_ESC_CHAR, A_ESC_START, A_HEX, A_HEX_LAST, A_LIT_END, A_LIT_START, A_LOW_HEX_LAST (+81 more)

### Community 3 - "Lexer Errors & Positions"
Cohesion: 0.05
Nodes (49): at(), columna_satura_en_u32_max(), LexErrorKind, Aborted, ControlCharInString, DoctypeForbidden, InvalidCharRef, InvalidEscape (+41 more)

### Community 4 - "Test Corpus Harness"
Cohesion: 0.07
Nodes (50): Error, Lex, Sink, LexError, CAMPOS, CAMPOS_EXPECTED, Case, CATEGORIAS (+42 more)

### Community 5 - "UTF-8 Tests & Cycle Graph"
Cohesion: 0.06
Nodes (33): Application, ChildInstance, application(), None, SameInstance, check_cycles(), clasificacion_completa(), FRAGMENT_BYTES (+25 more)

### Community 6 - "Schema Keyword Enum"
Cohesion: 0.04
Nodes (44): Kw, AdditionalProperties, AllOf, Anchor, AnyOf, Comment, Const, Default (+36 more)

### Community 7 - "Schema Parser Core"
Cohesion: 0.13
Nodes (16): hint(), is_schema_first(), is_str(), is_value_first(), Parser<'a>, Tok, Colon, Comma (+8 more)

### Community 8 - "XML Lexer Tests"
Cohesion: 0.08
Nodes (26): assert_error(), bloques_de_1_7_y_4096_bytes(), bom_inicial(), caracteres_invalidos(), Collect, doctype_y_entidades_prohibidas(), error(), errores_de_marcado() (+18 more)

### Community 9 - "JSON Token Events"
Cohesion: 0.08
Nodes (27): Event, BeginArray, BeginObject, Colon, Comma, EndArray, EndObject, False (+19 more)

### Community 10 - "CLI schemac Check"
Cohesion: 0.08
Nodes (27): analizar(), check(), error(), main(), STACK_SIZE, USO, archivo_inexistente(), errores_de_fase_2_y_3() (+19 more)

### Community 11 - "Symbol Table & Pointers"
Cohesion: 0.10
Nodes (20): array_index(), At, List, Map, Schema, emit(), Index, Node (+12 more)

### Community 12 - "JSON Lexer Tests"
Cohesion: 0.09
Nodes (20): atoms(), bloques_de_1_7_y_4096_bytes(), bom_inicial(), bytes_fuera_de_cadena(), cadenas_invalidas(), Collect, error(), fragment() (+12 more)

### Community 13 - "Parser Tests"
Cohesion: 0.10
Nodes (22): anidado(), anidamiento_enorme_no_desborda(), check_no_panic(), clave_repetida_al_final_de_objeto_grande(), clave_repetida_escapada(), claves_repetidas(), err(), escapes_decodificados() (+14 more)

### Community 14 - "UTF-8 DFA Table"
Cohesion: 0.06
Nodes (27): ACCEPT, C_80_8F, C_90_9F, C_A0_BF, C_ASCII, C_C2_DF, C_E0, C_E1_EF (+19 more)

### Community 15 - "Exact Number Lexemes"
Cohesion: 0.11
Nodes (22): casos_conocidos(), classify(), exponent(), info(), lexema(), NumInfo, to_i64(), decode() (+14 more)

### Community 16 - "Issue Creation Script"
Cohesion: 0.14
Nodes (14): asegurar_etiqueta(), cuerpo(), leer_tareas(), main(), pedir(), titulo(), titulos_existentes(), calcular() (+6 more)

### Community 17 - "Cycle Detection Tests"
Cohesion: 0.14
Nodes (20): cadena(), cadena_larga_de_referencias(), ciclo_a_la_profundidad_maxima(), circulares_de_la_tabla_se_rechazan(), componentes_con_ciclo(), componentes_encadenadas_con_su_camino_completo(), cycles(), Envoltura (+12 more)

### Community 18 - "XML Tokenizer DFA Diagram"
Cohesion: 0.18
Nodes (18): AFD del tokenizador XML (diagrama), AFD UTF-8 (afd-utf8), Atributos y valores entre comillas (s6-s11), Seccion CDATA <![CDATA[ ]]> (s23-s31), Referencia de caracter &#dec; / &#xhex; (s34-s37), Comentario <!-- --> (s18-s22), Estado contenido (s0) y deteccion de ]]> en texto (s1, s2), Etiqueta de cierre </nombre> (s13-s15) (+10 more)

### Community 19 - "Phase 1 Lexical Design"
Cohesion: 0.16
Nodes (15): Bitmask/counter in stack frame (required, minItems, xs:all), pattern compiled to DFA without backtracking (ReDoS mitigation), XML payload rules (UTF-8, no DOCTYPE/DTD, predefined entities only), Fase 1 - Lexical analysis, Bounded register (surrogate pairs, entity names) keeps language regular, Byte classes (refinement of delta-equivalence), CLASS[256], DOCTYPE rejection (DoctypeForbidden) against XXE and billion laughs, JSON tokenizer DFA (32 states, 31 classes) (+7 more)

### Community 20 - "Project Architecture & Crates"
Cohesion: 0.16
Nodes (15): runtime-c: C11 support headers for generated validator, infra: infrastructure as code and cloud deployment (Phase 7, pending), Project README: Validador de Esquemas JSON/XML con Automatas de Pila Ponderados, AST/DOM-based validator baseline for comparison, Compiler pipeline: lexico -> sintactico -> semantico -> automata -> codigo, crate automaton (Phase 4), crate cli (schemac command-line tool), crate codegen (Phase 5) (+7 more)

### Community 21 - "Keyword Value Classes"
Cohesion: 0.13
Nodes (14): Class, Bool, DependentRequired, Number, Other, Required, Schema, SchemaList (+6 more)

### Community 22 - "Unsupported Keyword Rejection"
Cohesion: 0.16
Nodes (12): classify(), CONTAINS, CONTAINS_COUNT, CONTENT, distance(), distancia_de_edicion(), DYNAMIC, IF (+4 more)

### Community 23 - "JSON Tokenizer DFA Diagram"
Cohesion: 0.18
Nodes (12): AFD UTF-8 (afd-utf8), AFD del tokenizador JSON (RFC 8259) diagram, Estado de error implícito (transiciones omitidas), Error InvalidUtf8, Literales true/false/null (s1-s10), Error LoneSurrogate, Subautómata de números (signo, int, frac, exp; s11-s18), RFC 8259 (JSON) (+4 more)

### Community 24 - "Ref Resolution Problems"
Cohesion: 0.17
Nodes (9): RefProblem, IntoLiteral, InvalidAnchor, NotASchema, NotFound, NotLocal, PercentEncoding, PointerEscape (+1 more)

### Community 25 - "Phase 2 Pending Items"
Cohesion: 0.20
Nodes (8): MAX_DEPTH_LIMIT = 256 / parse_with_max_depth, Prueba muchas_claves_en_tiempo_lineal (#[ignore]), Restricciones XML fuera de la gramatica, Pendientes de la Fase 2, Crate bench (criterion), Limite de tamano del esquema (servidor), Unicidad de cadenas no es libre de contexto, Camino pi(s) inyectivo y puntero JSON

### Community 26 - "DFA Transition Tables"
Cohesion: 0.27
Nodes (10): build_table(), ERR, TABLE, fill(), set(), Tr, tr(), build_table() (+2 more)

### Community 27 - "UTF-8 Decoder Steps"
Cohesion: 0.31
Nodes (6): Step, Char, Invalid, Pending, Utf8Decoder, validate()

### Community 28 - "Scope & Supported Subset"
Cohesion: 0.22
Nodes (10): Alcance: supported subset of JSON Schema and XSD, Bounded determinization (T3.7, 4096 states), Combinators allOf/anyOf/oneOf/not via product, bounded determinization or deferred check, Default limits (depth 256, string 1 MiB, number lexeme 1024, sv_limits_t), Deferred verification fallback, JSON payload rules (RFC 8259, UTF-8 only, duplicate keys, exact decimal lexeme), Out-of-scope keywords (if/then/else, contains, patternProperties, $dynamicRef, ...), Pending Phase 3 decisions (not complement, additionalProperties+allOf, xs:sequence vs objects) (+2 more)

### Community 29 - "JSON CFG & LL(1)"
Cohesion: 0.29
Nodes (11): Fase 2: Gramatica libre de contexto de JSON, Eliminacion de recursion izquierda, Factorizacion por la izquierda, Gramatica LL(1) de JSON (json.ll1), Gramatica JSON natural (RFC 8259, no LL(1)), JSON no es regular (lema de bombeo), scripts/ll1.py (generador FIRST/FOLLOW/tabla), Informe LL(1) JSON (+3 more)

### Community 30 - "Semantic Error Kinds"
Cohesion: 0.20
Nodes (7): ErrorKind, DuplicateAnchor, InvalidValue, NonProductiveCycle, UnresolvedRef, UnsupportedForm, UnsupportedKeyword

### Community 31 - "Weighted DPDA & Test Theory"
Cohesion: 0.22
Nodes (9): Property-test oracles (serde_json, random XML, from_utf8, trozos_arbitrarios), Pumping tests u v^k x y^k z, Test case categories: valid, invalid, malformed, non-dpda, pumping, extreme, Tropical semiring (N u {inf}, min, +, inf, 0), Weighted deterministic pushdown automaton M=(Q,Sigma,Gamma,delta,q0,Z0,F,w), Test case JSON format (_id, category, format, schema, payload, expected), expected block (valid, violations, errorKind, pointer, offset), Payload generator for pumping/extreme cases (+1 more)

### Community 32 - "Schema Metalanguage Grammar"
Cohesion: 0.29
Nodes (8): Fase 2: Gramatica del metalenguaje de esquemas, K_OTHER (palabras clave fuera del subconjunto), Clases de palabra clave K_*, Parser descendente recursivo (json_schema.rs), Gramatica LL(1) del esquema (esquema.ll1), Condiciones LL(1) (FIRST/FOLLOW disjuntos), Informe LL(1) gramatica de esquemas, Tabla LL(1) del esquema (sin conflictos)

### Community 33 - "Phase 3 Symbols & Refs"
Cohesion: 0.24
Nodes (7): Fase 3: Tabla de simbolos y resolucion de $ref, Automata determinista de recorrido del puntero, SymbolTable::fragment_clipped, Grafo de referencias G=(N, E_estr U E_ref), RefProblem (motivos de reference), Resolucion de $ref (RFC 3986 + RFC 6901), Tabla de simbolos T(S)=(N,id,padre,A,D,rho)

### Community 34 - "Repo Templates & NoSQL Corpus"
Cohesion: 0.22
Nodes (9): Plan task issue template (Tx.y), Pull request template, Non-productive $ref cycle error (T3.4), Local $ref resolution with productive recursion, Plan task Tx.y with Criterio de terminado (CT), NoSQL cloud persistence of test cases, Test cases README (corpus format), Metrics stored only in NoSQL by runner (T8.9) (+1 more)

### Community 35 - "CI & Streaming Invariants"
Cohesion: 0.25
Nodes (7): CI workflow (GitHub Actions), CI job: coverage with cargo llvm-cov, CI job: docs-check (generated .dot in sync), CI job: lint (rustfmt, clippy, clang-format, lexer wasm32 build), CI job: tests on Linux and Windows, Streaming O(n) time, O(1) memory, resumable, no_std, Makefile targets: lint, test, coverage, docs, docs-check

### Community 36 - "Meta-Schema Validation"
Cohesion: 0.25
Nodes (6): ParseError { kind, at }, Fase 3: Validacion contra meta-esquema, Categorias de palabras rechazadas (unsupported.rs), Meta-esquema 2020-12 restringido M', Numeros exactos sobre el lexema (number.rs), SemanticError { kind, at }

### Community 37 - "Non-Productive Cycles Theory"
Cohesion: 0.33
Nodes (8): Parser LL(1) como automata de pila, Fase 3: Ciclos no productivos, Camino mas corto por BFS desde el $ref representante, Ciclo no productivo, Clasificacion application (SameInstance/ChildInstance/None), Cota de pila O(|N|*d) = O(d), Grafo G0 de aristas que no consumen entrada, Tarjan iterativo (SCC) con CSR

### Community 38 - "DOT Diagram Export"
Cohesion: 0.43
Nodes (4): Dfa, escape(), list(), write()

### Community 39 - "Byte Class Partition Tests"
Cohesion: 0.32
Nodes (7): error_kind(), particion_de_clases(), distintas(), en_lexico_md(), particion_de_clases(), error_kind(), particion_de_clases()

### Community 40 - "Keyword Categories"
Cohesion: 0.25
Nodes (7): Category, Annotation, B, C, Core, Legacy, Unknown

### Community 41 - "XML Grammar & Non-CF Proof"
Cohesion: 0.39
Nodes (7): Fase 2: Gramatica de XML bien formado, XML bien formado no es libre de contexto, Lema de bombeo para lenguajes libres de contexto, Gramatica LL(1) de XML (xml.ll1), Terminales XML (STAG, ATTR, ETAG, TEXT, PI), Informe LL(1) XML, Tabla LL(1) de XML (sin conflictos)

### Community 42 - "Workspace Crates"
Cohesion: 0.43
Nodes (8): automaton, bench, cli, codegen, lexer, schema-parser, semantic, server

### Community 43 - "UTF-8 DFA Diagram"
Cohesion: 0.33
Nodes (7): Continuation states s2/s3/s4 (falta 1/2/3 bytes 80-BF), AFD UTF-8 (RFC 3629) diagram, Implicit error state (omitted transitions), Restricted lead-byte states E0/ED/F0/F4, RFC 3629 (UTF-8), s0 inicio (accepting start state; ASCII 00-7F loops), UTF-8 validation DFA

### Community 44 - "Prettier Config"
Cohesion: 0.40
Nodes (4): printWidth, semi, singleQuote, trailingComma

## Knowledge Gaps
- **212 isolated node(s):** `BeginObject`, `EndObject`, `BeginArray`, `EndArray`, `Colon` (+207 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 580 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **15 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Pos` connect `Lexer Errors & Positions` to `XML Lexer Actions`, `JSON Value AST & Classify`, `JSON Lexer Actions`, `Test Corpus Harness`, `UTF-8 Tests & Cycle Graph`, `JSON Token Events`, `Symbol Table & Pointers`, `Semantic Error Kinds`?**
  _High betweenness centrality (0.170) - this node is a cross-community bridge._
- **What connects `BeginObject`, `EndObject`, `BeginArray` to the rest of the system?**
  _212 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `XML Lexer Actions` be split into smaller, more focused modules?**
  _Cohesion score 0.01834862385321101 - nodes in this community are weakly interconnected._
- **Why does `Span` connect `JSON Value AST & Classify` to `XML Lexer Actions`, `JSON Lexer Actions`, `Lexer Errors & Positions`, `Test Corpus Harness`, `UTF-8 Tests & Cycle Graph`, `Schema Parser Core`, `XML Lexer Tests`, `JSON Lexer Tests`?**
  _High betweenness centrality (0.150) - this node is a cross-community bridge._
- **Should `JSON Value AST & Classify` be split into smaller, more focused modules?**
  _Cohesion score 0.05147563486616335 - nodes in this community are weakly interconnected._
- **Why does `Value` connect `Test Corpus Harness` to `JSON Value AST & Classify`, `UTF-8 Tests & Cycle Graph`, `Schema Parser Core`, `XML Lexer Tests`, `CLI schemac Check`, `Symbol Table & Pointers`, `JSON Lexer Tests`, `Parser Tests`, `Exact Number Lexemes`, `Cycle Detection Tests`?**
  _High betweenness centrality (0.113) - this node is a cross-community bridge._
- **Should `JSON Lexer Actions` be split into smaller, more focused modules?**
  _Cohesion score 0.021739130434782608 - nodes in this community are weakly interconnected._