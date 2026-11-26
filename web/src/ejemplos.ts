// Esquemas de ejemplo para la demo. Los primeros de cada formato son muy
// cortos, para que el autómata se lea de un vistazo; los últimos, más
// realistas. Cada uno trae payloads de muestra (válido, con violaciones y
// malformado) que la web pone al elegirlo.

export type Grupo = "JSON" | "XSD" | "DTD";
export type TipoMuestra = "valido" | "violaciones" | "malformado";

export interface Muestra {
  tipo: TipoMuestra;
  /** Lo que se espera del DPDA, para la etiqueta del botón. */
  nota: string;
  payload: string;
}

export interface Ejemplo {
  nombre: string;
  grupo: Grupo;
  /** Una línea: qué muestra el ejemplo. */
  descripcion: string;
  texto: string;
  muestras: Muestra[];
}

export const NOMBRE_MUESTRA: Record<TipoMuestra, string> = {
  valido: "válido",
  violaciones: "con violaciones",
  malformado: "malformado",
};

const muestras = (valido: string, violaciones: [string, string], malformado: string): Muestra[] => [
  { tipo: "valido", nota: "peso 0", payload: valido },
  { tipo: "violaciones", nota: violaciones[1], payload: violaciones[0] },
  { tipo: "malformado", nota: "peso ∞", payload: malformado },
];

export const LISTA_DE_USUARIOS = `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "array",
  "items": {
    "type": "object",
    "properties": {
      "id": { "type": "integer", "minimum": 1 },
      "nombre": { "type": "string", "minLength": 1, "maxLength": 80 },
      "correo": { "type": "string" },
      "activo": { "type": "boolean" }
    },
    "required": ["id", "nombre"],
    "additionalProperties": false
  }
}
`;

const XS = 'xmlns:xs="http://www.w3.org/2001/XMLSchema"';

export const EJEMPLOS: Ejemplo[] = [
  // ── JSON Schema ──────────────────────────────────────────────────────────
  {
    nombre: "Booleano",
    grupo: "JSON",
    descripcion: "Un solo valor: el autómata más pequeño posible.",
    texto: `{ "type": "boolean" }
`,
    muestras: muestras("true", ['"sí"', "peso 1"], "tru"),
  },
  {
    nombre: "Entero en rango",
    grupo: "JSON",
    descripcion: "minimum y maximum: la comprobación va en la transición del número.",
    texto: `{ "type": "integer", "minimum": 0, "maximum": 10 }
`,
    muestras: muestras("7", ["42", "peso 1"], "7.e"),
  },
  {
    nombre: "Enumeración",
    grupo: "JSON",
    descripcion: "enum de cadenas: el valor se compara al cerrar la cadena.",
    texto: `{ "enum": ["rojo", "verde", "azul"] }
`,
    muestras: muestras('"verde"', ['"negro"', "peso 1"], '"verde'),
  },
  {
    nombre: "Arreglo acotado",
    grupo: "JSON",
    descripcion: "items y maxItems: cuenta elementos con un contador en el marco de la pila.",
    texto: `{
  "type": "array",
  "items": { "type": "number" },
  "maxItems": 3
}
`,
    muestras: muestras("[1, 2.5, 3]", ['[1, "dos", 3, 4]', "peso 2"], "[1, 2,]"),
  },
  {
    nombre: "Objeto con requerido",
    grupo: "JSON",
    descripcion: "properties y required: un marco por objeto con los campos vistos.",
    texto: `{
  "type": "object",
  "properties": { "id": { "type": "integer" } },
  "required": ["id"]
}
`,
    muestras: muestras('{"id": 7}', ['{"id": "siete"}', "peso 1"], '{"id": 7'),
  },
  {
    nombre: "Usuario",
    grupo: "JSON",
    descripcion: "Un objeto realista: tipos, longitudes y sin propiedades extra.",
    texto: `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "properties": {
    "id": { "type": "integer", "minimum": 1 },
    "nombre": { "type": "string", "minLength": 1, "maxLength": 80 },
    "correo": { "type": "string" },
    "activo": { "type": "boolean" }
  },
  "required": ["id", "nombre"],
  "additionalProperties": false
}
`,
    muestras: muestras(
      '{"id": 7, "nombre": "Ana", "activo": true}',
      ['{"id": 0, "nombre": "", "edad": 30}', "peso 3"],
      '{"id": 7, "nombre": "Ana",}',
    ),
  },
  {
    nombre: "Árbol recursivo",
    grupo: "JSON",
    descripcion: "$ref recursivo: la pila de M guarda un marco por nivel del árbol.",
    texto: `{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$ref": "#/$defs/nodo",
  "$defs": {
    "nodo": {
      "type": "object",
      "properties": {
        "valor": { "type": "integer" },
        "hijos": { "type": "array", "items": { "$ref": "#/$defs/nodo" } }
      },
      "required": ["valor"]
    }
  }
}
`,
    muestras: muestras(
      '{"valor": 1, "hijos": [{"valor": 2}, {"valor": 3, "hijos": [{"valor": 4}]}]}',
      ['{"valor": 1, "hijos": [{"valor": "dos"}, {"hijos": []}]}', "peso 2"],
      '{"valor": 1, "hijos": [{"valor": 2}}',
    ),
  },
  {
    nombre: "Lista de usuarios",
    grupo: "JSON",
    descripcion: "Para la prueba de carga de la comparación en vivo.",
    texto: LISTA_DE_USUARIOS,
    muestras: muestras(
      '[{"id": 1, "nombre": "Ana"}, {"id": 2, "nombre": "Luis", "activo": false}]',
      ['[{"id": 1, "nombre": "Ana"}, {"id": 0, "nombre": "Luis"}]', "peso 1"],
      '[{"id": 1, "nombre": "Ana"}',
    ),
  },
  // ── XSD ──────────────────────────────────────────────────────────────────
  {
    nombre: "Elemento de texto",
    grupo: "XSD",
    descripcion: "Un elemento con texto: abrir, leer, cerrar.",
    texto: `<xs:schema ${XS}>
  <xs:element name="nota" type="xs:string"/>
</xs:schema>
`,
    muestras: muestras("<nota>hola</nota>", ["<aviso>hola</aviso>", "peso 1"], "<nota>hola</nota"),
  },
  {
    nombre: "Secuencia",
    grupo: "XSD",
    descripcion: "xs:sequence: el orden de los hijos son estados del autómata.",
    texto: `<xs:schema ${XS}>
  <xs:element name="persona">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="nombre" type="xs:string"/>
        <xs:element name="edad" type="xs:integer"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>
`,
    muestras: muestras(
      "<persona><nombre>Ana</nombre><edad>30</edad></persona>",
      ["<persona><nombre>Ana</nombre><edad>treinta</edad></persona>", "peso 1"],
      "<persona><nombre>Ana</edad></persona>",
    ),
  },
  {
    nombre: "Pedido",
    grupo: "XSD",
    descripcion: "Secuencia con opcional, decimal y atributo requerido.",
    texto: `<xs:schema ${XS}>
  <xs:element name="pedido">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="cliente" type="xs:string"/>
        <xs:element name="nota" type="xs:string" minOccurs="0"/>
        <xs:element name="total" type="xs:decimal"/>
      </xs:sequence>
      <xs:attribute name="id" type="xs:positiveInteger" use="required"/>
    </xs:complexType>
  </xs:element>
</xs:schema>
`,
    muestras: muestras(
      '<pedido id="7"><cliente>Ana</cliente><total>9.50</total></pedido>',
      ["<pedido><cliente>Ana</cliente><total>caro</total></pedido>", "peso 2"],
      '<pedido id="7"><cliente>Ana</cliente><total>9.50</total>',
    ),
  },
  // ── DTD ──────────────────────────────────────────────────────────────────
  {
    nombre: "Saludo",
    grupo: "DTD",
    descripcion: "Una declaración: la DTD más corta.",
    texto: `<!ELEMENT saludo (#PCDATA)>
`,
    muestras: muestras("<saludo>hola</saludo>", ["<saludo><b>hola</b></saludo>", "peso 1"], "<saludo>hola</saludo"),
  },
  {
    nombre: "Elección",
    grupo: "DTD",
    descripcion: "(a | b): una bifurcación determinista en el autómata.",
    texto: `<!ELEMENT pago (tarjeta | efectivo)>
<!ELEMENT tarjeta (#PCDATA)>
<!ELEMENT efectivo EMPTY>
`,
    muestras: muestras(
      "<pago><efectivo/></pago>",
      ["<pago><tarjeta>1234</tarjeta><efectivo/></pago>", "peso 1"],
      "<pago><efectivo></pago>",
    ),
  },
  {
    nombre: "Pedido",
    grupo: "DTD",
    descripcion: "Secuencia, elección, repetición, opcional y atributos con valores fijos.",
    texto: `<!-- DTD externa (D86): la raíz puede ser cualquier elemento declarado. -->
<!ELEMENT pedido (cliente, (tarjeta | efectivo), item+, nota?)>
<!ELEMENT cliente (#PCDATA)>
<!ELEMENT tarjeta (#PCDATA)>
<!ELEMENT efectivo EMPTY>
<!ELEMENT item (#PCDATA)>
<!ELEMENT nota (#PCDATA)>
<!ATTLIST pedido id CDATA #REQUIRED
                 estado (nuevo|pagado|enviado) "nuevo">
<!ATTLIST item sku CDATA #REQUIRED>
`,
    muestras: muestras(
      '<pedido id="7" estado="pagado"><cliente>Ana</cliente><efectivo/><item sku="A1">lápiz</item></pedido>',
      ['<pedido estado="perdido"><cliente>Ana</cliente><efectivo/><item sku="A1">lápiz</item></pedido>', "peso 2"],
      '<pedido id="7"><cliente>Ana</cliente><efectivo/><item sku="A1">lápiz</item>',
    ),
  },
  {
    nombre: "Con errores",
    grupo: "JSON",
    descripcion: "Un esquema inválido: el compilador marca cada error en el editor.",
    texto: `{
  "type": ["", "x"],
  "minItems": -1,
  "properties": { "a": { "$ref": "#/$defs/no-existe" } }
}
`,
    muestras: [],
  },
];

/**
 * Prueba de carga del panel de comparación: `n` usuarios válidos para
 * «Lista de usuarios» salvo uno, sembrado a tres cuartos del arreglo con
 * `"id": 0` (viola `minimum: 1`): una violación, con su byte, en un payload
 * grande, para ver la memoria fija del DPDA frente al árbol de la línea base.
 * Compacto (unos 80 bytes por usuario): 10 000 caben con holgura en el tope de
 * subida del servidor (1 MiB, `MAX_UPLOAD_BYTES`).
 */
export function cargaDeUsuarios(n: number): string {
  const malo = Math.floor((n * 3) / 4);
  const filas: string[] = [];
  for (let i = 1; i <= n; i++) {
    const id = i === malo ? 0 : i;
    filas.push(`{"id":${id},"nombre":"Usuario ${i}","correo":"u${i}@ejemplo.org","activo":${i % 3 !== 0}}`);
  }
  return `[\n${filas.join(",\n")}\n]\n`;
}
