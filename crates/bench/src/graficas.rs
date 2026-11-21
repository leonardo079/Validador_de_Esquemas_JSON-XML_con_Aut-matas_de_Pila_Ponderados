//! E1 — Gráficas de bombeo en SVG (D80; `docs/fase6/bombeo.md`): tiempo,
//! pila y memoria contra k y n, a partir del CSV de
//! `metricas --bombeo-fino --csv`. SVG escrito a mano (sin crates de
//! gráficos): ejes lineales o logarítmicos, marcas, rejilla y leyenda.

/// Una serie: nombre, color y puntos (x, y).
#[derive(Clone, Debug)]
pub struct Serie {
    pub nombre: String,
    pub color: &'static str,
    pub puntos: Vec<(f64, f64)>,
    /// Línea discontinua (para referencias teóricas).
    pub discontinua: bool,
}

/// Escala de un eje.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escala {
    Lineal,
    Log,
}

/// Una gráfica.
#[derive(Clone, Debug)]
pub struct Grafica {
    pub titulo: String,
    pub eje_x: String,
    pub eje_y: String,
    pub escala_x: Escala,
    pub escala_y: Escala,
    pub series: Vec<Serie>,
}

/// Colores de las series (azul para el DPDA, naranja para la línea base,
/// gris para referencias).
pub const AZUL: &str = "#2f5bd3";
pub const NARANJA: &str = "#d9730d";
pub const VERDE: &str = "#1f7a3d";
pub const GRIS: &str = "#8a8a93";

const ANCHO: f64 = 720.0;
const ALTO: f64 = 420.0;
const IZQ: f64 = 84.0;
const DER: f64 = 24.0;
const ARR: f64 = 48.0;
const ABA: f64 = 64.0;

fn escapar(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Texto compacto de un número para una marca del eje.
pub fn rotulo(v: f64) -> String {
    let a = v.abs();
    if a == 0.0 {
        return "0".into();
    }
    if a >= 1e6 {
        let e = a.log10().floor() as i32;
        let m = v / 10f64.powi(e);
        return if (m - m.round()).abs() < 1e-9 && m.round() == 1.0 {
            format!("10^{e}")
        } else {
            format!("{}e{e}", recortar(m))
        };
    }
    if a >= 1.0 {
        return recortar(v);
    }
    if a >= 1e-3 {
        return recortar(v);
    }
    format!("{v:.0e}")
}

fn recortar(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_owned()
}

/// Marcas "redondas" de un intervalo lineal (≈ 5).
/// Paso "redondo" (1, 2, 2,5 o 5 por una potencia de 10) para ≈ 5 marcas.
fn paso_lineal(min: f64, max: f64) -> f64 {
    let paso_bruto = (max - min).max(f64::MIN_POSITIVE) / 5.0;
    let mag = 10f64.powf(paso_bruto.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|f| f * mag)
        .find(|p| *p >= paso_bruto)
        .unwrap_or(10.0 * mag)
}

fn marcas_lineales(min: f64, max: f64) -> Vec<f64> {
    let paso = paso_lineal(min, max);
    let mut v = Vec::new();
    let mut t = (min / paso).ceil() * paso;
    while t <= max + paso * 1e-9 {
        v.push(if t.abs() < paso * 1e-9 { 0.0 } else { t });
        t += paso;
    }
    v
}

/// Potencias de 10 que caen en el intervalo (logarítmico).
fn marcas_log(min: f64, max: f64) -> Vec<f64> {
    let (a, b) = (min.log10().floor() as i32, max.log10().ceil() as i32);
    (a..=b)
        .map(|e| 10f64.powi(e))
        .filter(|&t| t >= min * 0.999 && t <= max * 1.001)
        .collect()
}

/// Intervalo de un eje con margen; en log, potencias de 10 enteras.
fn intervalo(valores: &[f64], escala: Escala) -> (f64, f64) {
    let validos: Vec<f64> = valores
        .iter()
        .copied()
        .filter(|v| v.is_finite() && (escala == Escala::Lineal || *v > 0.0))
        .collect();
    let (mut min, mut max) = validos
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
            (a.min(v), b.max(v))
        });
    if !min.is_finite() {
        return (0.0, 1.0);
    }
    match escala {
        Escala::Log => (
            10f64.powf(min.log10().floor()),
            10f64
                .powf(max.log10().ceil())
                .max(10f64.powf(min.log10().floor() + 1.0)),
        ),
        Escala::Lineal => {
            min = min.min(0.0);
            if max <= min {
                max = min + 1.0;
            }
            // Redondeo hacia ARRIBA a una marca: ningún punto queda fuera.
            let paso = paso_lineal(min, max);
            (min, (max / paso).ceil() * paso)
        }
    }
}

fn proyectar(v: f64, (min, max): (f64, f64), escala: Escala, a: f64, b: f64) -> f64 {
    let t = match escala {
        Escala::Lineal => (v - min) / (max - min),
        Escala::Log => (v.log10() - min.log10()) / (max.log10() - min.log10()),
    };
    a + t * (b - a)
}

impl Grafica {
    /// El SVG completo. Colores por variables CSS con modo oscuro.
    pub fn svg(&self) -> String {
        let xs: Vec<f64> = self
            .series
            .iter()
            .flat_map(|s| s.puntos.iter().map(|p| p.0))
            .collect();
        let ys: Vec<f64> = self
            .series
            .iter()
            .flat_map(|s| s.puntos.iter().map(|p| p.1))
            .collect();
        let rx = intervalo(&xs, self.escala_x);
        let ry = intervalo(&ys, self.escala_y);
        let (x0, x1, y0, y1) = (IZQ, ANCHO - DER, ALTO - ABA, ARR);
        let px = |v| proyectar(v, rx, self.escala_x, x0, x1);
        let py = |v| proyectar(v, ry, self.escala_y, y0, y1);
        let mut o = String::new();
        o.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {ANCHO} {ALTO}\" width=\"{ANCHO}\" height=\"{ALTO}\" role=\"img\" aria-label=\"{}\" font-family=\"system-ui, -apple-system, Segoe UI, sans-serif\">\n",
            escapar(&self.titulo)
        ));
        o.push_str("<style>.f{fill:#ffffff}.t{fill:#1d1d1f}.m{fill:#6b6b72}.r{stroke:#e4e4e8}.e{stroke:#6b6b72}@media (prefers-color-scheme: dark){.f{fill:#1f1f24}.t{fill:#ececf1}.m{fill:#a0a0aa}.r{stroke:#34343c}.e{stroke:#a0a0aa}}</style>\n");
        o.push_str(&format!(
            "<rect class=\"f\" width=\"{ANCHO}\" height=\"{ALTO}\"/>\n"
        ));
        o.push_str(&format!(
            "<text class=\"t\" x=\"{IZQ}\" y=\"28\" font-size=\"16\" font-weight=\"600\">{}</text>\n",
            escapar(&self.titulo)
        ));
        let marcas_x = match self.escala_x {
            Escala::Lineal => marcas_lineales(rx.0, rx.1),
            Escala::Log => marcas_log(rx.0, rx.1),
        };
        let marcas_y = match self.escala_y {
            Escala::Lineal => marcas_lineales(ry.0, ry.1),
            Escala::Log => marcas_log(ry.0, ry.1),
        };
        for &t in &marcas_y {
            let y = py(t);
            o.push_str(&format!(
                "<line class=\"r\" x1=\"{x0}\" x2=\"{x1}\" y1=\"{y:.1}\" y2=\"{y:.1}\"/>\n"
            ));
            o.push_str(&format!(
                "<text class=\"m\" x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" text-anchor=\"end\">{}</text>\n",
                x0 - 8.0,
                y + 4.0,
                escapar(&rotulo(t))
            ));
        }
        for &t in &marcas_x {
            let x = px(t);
            o.push_str(&format!(
                "<line class=\"r\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{y1}\" y2=\"{y0}\"/>\n"
            ));
            o.push_str(&format!(
                "<text class=\"m\" x=\"{x:.1}\" y=\"{:.1}\" font-size=\"11\" text-anchor=\"middle\">{}</text>\n",
                y0 + 18.0,
                escapar(&rotulo(t))
            ));
        }
        o.push_str(&format!(
            "<line class=\"e\" x1=\"{x0}\" x2=\"{x1}\" y1=\"{y0}\" y2=\"{y0}\"/>\n"
        ));
        o.push_str(&format!(
            "<line class=\"e\" x1=\"{x0}\" x2=\"{x0}\" y1=\"{y1}\" y2=\"{y0}\"/>\n"
        ));
        o.push_str(&format!(
            "<text class=\"t\" x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" text-anchor=\"middle\">{}</text>\n",
            (x0 + x1) / 2.0,
            ALTO - 18.0,
            escapar(&self.eje_x)
        ));
        o.push_str(&format!(
            "<text class=\"t\" font-size=\"12\" text-anchor=\"middle\" transform=\"translate(18 {:.1}) rotate(-90)\">{}</text>\n",
            (y0 + y1) / 2.0,
            escapar(&self.eje_y)
        ));
        for s in &self.series {
            let pts: Vec<String> = s
                .puntos
                .iter()
                .filter(|(x, y)| {
                    x.is_finite()
                        && y.is_finite()
                        && (self.escala_x == Escala::Lineal || *x > 0.0)
                        && (self.escala_y == Escala::Lineal || *y > 0.0)
                })
                .map(|&(x, y)| format!("{:.1},{:.1}", px(x), py(y)))
                .collect();
            if pts.is_empty() {
                continue;
            }
            let guion = if s.discontinua {
                " stroke-dasharray=\"6 4\""
            } else {
                ""
            };
            o.push_str(&format!(
                "<polyline fill=\"none\" stroke=\"{}\" stroke-width=\"2\"{guion} points=\"{}\"/>\n",
                s.color,
                pts.join(" ")
            ));
            if !s.discontinua {
                for p in &pts {
                    let (x, y) = p.split_once(',').unwrap_or(("0", "0"));
                    o.push_str(&format!(
                        "<circle cx=\"{x}\" cy=\"{y}\" r=\"3\" fill=\"{}\"/>\n",
                        s.color
                    ));
                }
            }
        }
        // Leyenda arriba a la derecha, sobre un fondo para que ninguna línea
        // la cruce.
        o.push_str(&format!(
            "<rect class=\"f\" x=\"{:.1}\" y=\"{:.1}\" width=\"262\" height=\"{:.1}\" opacity=\"0.92\" rx=\"4\"/>
",
            x1 - 268.0,
            ARR - 6.0,
            18.0 * self.series.len() as f64 + 6.0
        ));
        let mut ly = ARR + 6.0;
        for s in &self.series {
            let lx = x1 - 260.0;
            let guion = if s.discontinua {
                " stroke-dasharray=\"6 4\""
            } else {
                ""
            };
            o.push_str(&format!(
                "<line x1=\"{lx}\" x2=\"{:.1}\" y1=\"{ly:.1}\" y2=\"{ly:.1}\" stroke=\"{}\" stroke-width=\"2\"{guion}/>\n",
                lx + 22.0,
                s.color
            ));
            o.push_str(&format!(
                "<text class=\"t\" x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\">{}</text>\n",
                lx + 28.0,
                ly + 4.0,
                escapar(&s.nombre)
            ));
            ly += 18.0;
        }
        o.push_str("</svg>\n");
        o
    }
}

/// Una fila del CSV de `bench::csv`.
#[derive(Clone, Debug, PartialEq)]
pub struct Fila {
    pub caso: String,
    pub validador: String,
    pub bytes: f64,
    pub profundidad: f64,
    pub segundos: f64,
    pub pico_bytes: f64,
    pub asignaciones: f64,
}

/// Lee el CSV de `bench::csv` (el primer campo entre comillas).
pub fn leer_csv(texto: &str) -> Result<Vec<Fila>, String> {
    let mut l = texto.lines();
    if !l.next().unwrap_or("").starts_with("caso,validador,bytes") {
        return Err("no es el CSV de `metricas --csv`".into());
    }
    let mut v = Vec::new();
    for (i, linea) in l.enumerate().filter(|(_, x)| !x.trim().is_empty()) {
        let resto = linea
            .strip_prefix('"')
            .ok_or(format!("línea {}: sin comillas", i + 2))?;
        let fin = resto
            .find("\",")
            .ok_or(format!("línea {}: caso sin cerrar", i + 2))?;
        let caso = resto[..fin].replace("\"\"", "\"");
        let c: Vec<&str> = resto[fin + 2..].split(',').collect();
        if c.len() != 12 {
            return Err(format!("línea {}: {} campos", i + 2, c.len() + 1));
        }
        let n = |s: &str| s.parse::<f64>().unwrap_or(f64::NAN);
        v.push(Fila {
            caso,
            validador: c[0].to_owned(),
            bytes: n(c[1]),
            profundidad: n(c[2]),
            segundos: n(c[5]),
            asignaciones: n(c[8]),
            pico_bytes: n(c[9]),
        });
    }
    Ok(v)
}

/// El parámetro k o n de un nombre de familia de bombeo (`…, k=10`).
pub fn parametro(caso: &str) -> Option<f64> {
    caso.rsplit_once('=')
        .and_then(|(_, n)| n.trim().parse().ok())
}

/// Puntos (parámetro, f(fila)) de una familia y un validador, ordenados.
pub fn puntos(
    filas: &[Fila],
    prefijo: &str,
    validador: &str,
    f: impl Fn(&Fila) -> f64,
) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = filas
        .iter()
        .filter(|r| r.caso.starts_with(prefijo) && r.validador == validador)
        .filter_map(|r| Some((parametro(&r.caso)?, f(r))))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}
