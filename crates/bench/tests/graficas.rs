//! Pruebas del dibujo de gráficas (E1, D80). Deterministas (D1).

use bench::graficas::{leer_csv, parametro, puntos, rotulo, Escala, Grafica, Serie, AZUL, NARANJA};

const CSV: &str = "caso,validador,bytes,profundidad,valido,violaciones,segundos,mb_s,ns_por_byte,asignaciones,pico_bytes,estados,transiciones
\"bombeo [^k ]^k, k=10\",DPDA,20,10,true,0,2e-6,10.0,100.0,1,73728,16,60
\"bombeo [^k ]^k, k=1\",DPDA,2,1,true,0,1e-6,2.0,500.0,1,73728,16,60
\"bombeo [^k ]^k, k=10\",línea base DOM,20,10,true,0,3e-6,6.6,150.0,13,1500,,
\"arreglo de objetos, n=100\",DPDA,801,2,true,0,2e-5,40.0,25.0,1,75776,27,94
";

#[test]
fn lee_el_csv_y_ordena_por_parametro() {
    let f = leer_csv(CSV).unwrap();
    assert_eq!(f.len(), 4);
    assert_eq!(f[0].caso, "bombeo [^k ]^k, k=10");
    assert_eq!(parametro(&f[0].caso), Some(10.0));
    assert_eq!(parametro("arreglo de objetos, n=100"), Some(100.0));
    let p = puntos(&f, "bombeo", "DPDA", |r| r.pico_bytes);
    assert_eq!(p, vec![(1.0, 73728.0), (10.0, 73728.0)]);
    assert_eq!(
        puntos(&f, "bombeo", "línea base DOM", |r| r.asignaciones),
        vec![(10.0, 13.0)]
    );
    assert!(leer_csv("a,b\n1,2\n").is_err());
}

fn grafica(escala: Escala) -> Grafica {
    Grafica {
        titulo: "Prueba <&>".into(),
        eje_x: "n".into(),
        eje_y: "bytes".into(),
        escala_x: escala,
        escala_y: escala,
        series: vec![
            Serie {
                nombre: "DPDA".into(),
                color: AZUL,
                puntos: vec![(100.0, 7e4), (1e5, 7e4)],
                discontinua: false,
            },
            Serie {
                nombre: "línea base".into(),
                color: NARANJA,
                puntos: vec![(100.0, 6e4), (1e5, 6.4e7)],
                discontinua: true,
            },
        ],
    }
}

#[test]
fn svg_bien_formado_con_series_y_leyenda() {
    for escala in [Escala::Lineal, Escala::Log] {
        let s = grafica(escala).svg();
        assert!(s.starts_with("<svg ") && s.trim_end().ends_with("</svg>"));
        assert_eq!(s.matches("<polyline").count(), 2);
        // Puntos solo en la serie continua; la discontinua lleva guiones.
        assert_eq!(s.matches("<circle").count(), 2);
        assert!(s.contains("stroke-dasharray"));
        assert!(s.contains("Prueba &lt;&amp;&gt;"), "el título se escapa");
        assert!(s.contains(">DPDA<") && s.contains(">línea base<"));
        assert!(s.contains("prefers-color-scheme: dark"));
        // Ninguna coordenada NaN o infinita.
        assert!(!s.contains("NaN") && !s.contains("inf"));
    }
}

#[test]
fn eje_logaritmico_en_potencias_de_diez() {
    let s = grafica(Escala::Log).svg();
    for marca in [">100<", ">1000<", ">10^6<", ">10^7<"] {
        assert!(s.contains(marca), "falta la marca {marca}");
    }
}

#[test]
fn rotulos_compactos() {
    assert_eq!(rotulo(0.0), "0");
    assert_eq!(rotulo(250.0), "250");
    assert_eq!(rotulo(2.5), "2.5");
    assert_eq!(rotulo(1e6), "10^6");
    assert_eq!(rotulo(0.25), "0.25");
}

/// El eje lineal llega al menos al mayor valor (antes se redondeaba hacia
/// abajo y 73 728 o k = 255 quedaban fuera del dibujo).
#[test]
fn eje_lineal_cubre_el_maximo() {
    let g = Grafica {
        titulo: "t".into(),
        eje_x: "k".into(),
        eje_y: "bytes".into(),
        escala_x: Escala::Lineal,
        escala_y: Escala::Lineal,
        series: vec![Serie {
            nombre: "s".into(),
            color: AZUL,
            puntos: vec![(1.0, 10.0), (255.0, 73728.0)],
            discontinua: false,
        }],
    };
    let s = g.svg();
    // Todos los puntos dentro del área del dibujo (y ≥ 48, x ≤ 696).
    let i = s.find("points=\"").unwrap() + 8;
    for p in s[i..s[i..].find('"').unwrap() + i].split(' ') {
        let (x, y) = p.split_once(',').unwrap();
        let (x, y): (f64, f64) = (x.parse().unwrap(), y.parse().unwrap());
        assert!(x <= 696.0 + 1e-6 && y >= 48.0 - 1e-6, "{p}");
    }
    assert!(
        s.contains(">80000<") && s.contains(">300<") || s.contains(">260<") || s.contains(">250<")
    );
}
