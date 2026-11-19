// Esquemas de ejemplo para la demo (del corpus del repo, simplificados).

export const EJEMPLOS: { nombre: string; texto: string }[] = [
  {
    nombre: "JSON: usuario",
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
  },
  {
    nombre: "JSON: árbol recursivo",
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
  },
  {
    nombre: "XSD: pedido",
    texto: `<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
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
  },
  {
    nombre: "Con errores",
    texto: `{
  "type": ["", "x"],
  "minItems": -1,
  "properties": { "a": { "$ref": "#/$defs/no-existe" } }
}
`,
  },
];
