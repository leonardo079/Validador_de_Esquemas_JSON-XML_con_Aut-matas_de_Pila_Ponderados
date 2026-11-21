//! Payloads XML arbitrarios contra XSD fijos (F6.4): las propiedades de
//! [`comun::propiedades`]. Sin oráculo externo (libxml2 va tras una feature y
//! en Docker, F6.7b); la equivalencia con el C la da la reproducción en la
//! prueba diferencial (`scripts/fuzz.sh`).
#![no_main]

use std::sync::OnceLock;

use automaton::Automaton;
use libfuzzer_sys::fuzz_target;

#[path = "../comun/mod.rs"]
mod comun;

const ESQUEMAS: &[&str] = &[
    include_str!("../../tests/schemas/xsd/parse-ok/pedido.xsd"),
    r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r"><xs:complexType><xs:sequence>
    <xs:element name="d" minOccurs="0" maxOccurs="3"><xs:simpleType><xs:restriction base="xs:decimal"><xs:maxInclusive value="100"/></xs:restriction></xs:simpleType></xs:element>
    <xs:choice minOccurs="0" maxOccurs="unbounded"><xs:element name="s" type="xs:string"/><xs:element name="b" type="xs:boolean"/></xs:choice>
    <xs:element name="e" minOccurs="0"><xs:complexType><xs:all><xs:element name="x" type="xs:int"/><xs:element name="y" type="xs:double" minOccurs="0"/></xs:all></xs:complexType></xs:element>
  </xs:sequence>
  <xs:attribute name="a" use="required"><xs:simpleType><xs:restriction base="xs:string"><xs:pattern value="[a-c]{1,3}"/></xs:restriction></xs:simpleType></xs:attribute>
  <xs:attribute name="f" type="xs:string" fixed="1"/>
  </xs:complexType></xs:element>
</xs:schema>"#,
];

fn esquemas() -> &'static [Automaton] {
    static M: OnceLock<Vec<Automaton>> = OnceLock::new();
    M.get_or_init(|| {
        ESQUEMAS
            .iter()
            .map(|s| comun::m(comun::en_hilo(|| comun::ir_xsd(s.as_bytes()))))
            .collect()
    })
}

fuzz_target!(|p: &[u8]| {
    for m in esquemas() {
        comun::en_hilo(|| comun::propiedades(m, p));
    }
});
