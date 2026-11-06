//! Extraction of the "Icon" component set of `VLC Redesign (Community).fig`
//! into monochrome SVG layers (`<name>.svg` + `<name>.accent.svg`) consumed by
//! `ondrane/build.rs`.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;

use crate::kiwi::{decode_canvas, Value};

type Guid = (u64, u64);
type Mat = [f64; 6];

const IDENTITY: Mat = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

fn guid(v: &Value) -> Option<Guid> {
    Some((v.u("sessionID")?, v.u("localID")?))
}

fn mul(a: &Mat, b: &Mat) -> Mat {
    [
        a[0] * b[0] + a[1] * b[3],
        a[0] * b[1] + a[1] * b[4],
        a[0] * b[2] + a[1] * b[5] + a[2],
        a[3] * b[0] + a[4] * b[3],
        a[3] * b[1] + a[4] * b[4],
        a[3] * b[2] + a[4] * b[5] + a[5],
    ]
}

fn transform(n: &Value) -> Mat {
    let Some(t) = n.get("transform") else {
        return IDENTITY;
    };
    [
        t.f("m00").unwrap_or(1.0),
        t.f("m01").unwrap_or(0.0),
        t.f("m02").unwrap_or(0.0),
        t.f("m10").unwrap_or(0.0),
        t.f("m11").unwrap_or(1.0),
        t.f("m12").unwrap_or(0.0),
    ]
}

/// Trimmed float formatting (Python's `%g`-like for our value ranges).
fn g(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

struct Doc<'a> {
    nodes: Vec<&'a Value>,
    by_guid: HashMap<Guid, usize>,
    kids: HashMap<Guid, Vec<usize>>,
    blobs: Vec<&'a [u8]>,
}

impl<'a> Doc<'a> {
    fn new(msg: &'a Value) -> Self {
        let nodes: Vec<&Value> = msg.arr("nodeChanges").iter().collect();
        let mut by_guid = HashMap::new();
        let mut kids: HashMap<Guid, Vec<usize>> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            if let Some(gid) = n.get("guid").and_then(guid) {
                by_guid.insert(gid, i);
            }
            if let Some(parent) = n
                .get("parentIndex")
                .and_then(|p| p.get("guid"))
                .and_then(guid)
            {
                kids.entry(parent).or_default().push(i);
            }
        }
        for v in kids.values_mut() {
            v.sort_by(|a, b| {
                let pa = nodes[*a]
                    .get("parentIndex")
                    .and_then(|p| p.s("position"))
                    .unwrap_or("");
                let pb = nodes[*b]
                    .get("parentIndex")
                    .and_then(|p| p.s("position"))
                    .unwrap_or("");
                pa.cmp(pb)
            });
        }
        let blobs = msg
            .arr("blobs")
            .iter()
            .map(|b| b.bytes("bytes").unwrap_or(&[]))
            .collect();
        Self {
            nodes,
            by_guid,
            kids,
            blobs,
        }
    }

    fn children(&self, n: &Value) -> &[usize] {
        n.get("guid")
            .and_then(guid)
            .and_then(|g| self.kids.get(&g))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn style(&self, n: &Value, key: &str) -> Option<&'a Value> {
        let gid = n.get(key)?.get("guid").and_then(guid)?;
        self.by_guid.get(&gid).map(|i| self.nodes[*i])
    }

    /// Paints of a node, falling back to its shared style.
    fn paints(&self, n: &'a Value, key: &str, style_key: &str) -> Vec<&'a Value> {
        let list: &[Value] = if n.has(key) {
            n.arr(key)
        } else if let Some(s) = self.style(n, style_key) {
            s.arr("fillPaints")
        } else {
            &[]
        };
        list.iter()
            .filter(|p| p.b("visible").unwrap_or(true) && p.s("type") == Some("SOLID"))
            .collect()
    }

    fn path(&self, blob: u64, m: &Mat, scale: f64, ox: f64, oy: f64, sy: f64) -> String {
        let Some(d) = self.blobs.get(blob as usize) else {
            return String::new();
        };
        let mut out = String::new();
        let mut p = 0;
        let pt = |p: &mut usize| -> Option<String> {
            let x = f32::from_le_bytes(d.get(*p..*p + 4)?.try_into().ok()?) as f64;
            let y = f32::from_le_bytes(d.get(*p + 4..*p + 8)?.try_into().ok()?) as f64;
            *p += 8;
            let (x, y) = (x * scale + ox, y * sy + oy);
            Some(format!(
                "{:.3} {:.3}",
                m[0] * x + m[1] * y + m[2],
                m[3] * x + m[4] * y + m[5]
            ))
        };
        while p < d.len() {
            let c = d[p];
            p += 1;
            let seg = match c {
                0 => {
                    if !out.is_empty() {
                        out.push('Z');
                    }
                    continue;
                }
                1 => pt(&mut p).map(|a| format!("M{a}")),
                2 => pt(&mut p).map(|a| format!("L{a}")),
                3 => (|| Some(format!("Q{} {}", pt(&mut p)?, pt(&mut p)?)))(),
                4 => (|| Some(format!("C{} {} {}", pt(&mut p)?, pt(&mut p)?, pt(&mut p)?)))(),
                _ => None,
            };
            match seg {
                Some(s) => out.push_str(&s),
                None => break,
            }
        }
        out
    }
}

struct Shape {
    accent: bool,
    d: String,
    evenodd: bool,
    opacity: f64,
}

fn is_accent(paint: &Value, style_name: &str) -> bool {
    if style_name.contains("Accent") {
        return true;
    }
    let c = paint.get("color");
    let ch = |k: &str| c.and_then(|c| c.f(k)).unwrap_or(0.0);
    let (r, g, b) = (ch("r"), ch("g"), ch("b"));
    r.max(g).max(b) - r.min(g).min(b) > 0.15
}

fn collect<'a>(doc: &Doc<'a>, n: &'a Value, m: &Mat, op: f64, out: &mut Vec<Shape>) {
    if !n.b("visible").unwrap_or(true) {
        return;
    }
    let m = mul(m, &transform(n));
    let op = op * n.f("opacity").unwrap_or(1.0);
    let style_name = |key: &str| {
        doc.style(n, key)
            .and_then(|s| s.s("name"))
            .unwrap_or("")
            .to_string()
    };
    let fills = doc.paints(n, "fillPaints", "styleIdForFill");
    let strokes = doc.paints(n, "strokePaints", "styleIdForStrokeFill");
    let alpha = |p: &Value| {
        op * p.f("opacity").unwrap_or(1.0) * p.get("color").and_then(|c| c.f("a")).unwrap_or(1.0)
    };

    if n.s("type") == Some("TEXT") {
        if let Some(p) = fills.last() {
            let accent = is_accent(p, &style_name("styleIdForFill"));
            if let Some(dt) = n.get("derivedTextData") {
                for gl in dt.arr("glyphs") {
                    let (Some(blob), Some(size)) = (gl.u("commandsBlob"), gl.f("fontSize")) else {
                        continue;
                    };
                    let pos = gl.get("position");
                    let (x, y) = (
                        pos.and_then(|p| p.f("x")).unwrap_or(0.0),
                        pos.and_then(|p| p.f("y")).unwrap_or(0.0),
                    );
                    out.push(Shape {
                        accent,
                        d: doc.path(blob, &m, size, x, y, -size),
                        evenodd: false,
                        opacity: alpha(p),
                    });
                }
            }
        }
    }
    for (geo_key, paints, style_key) in [
        ("fillGeometry", &fills, "styleIdForFill"),
        ("strokeGeometry", &strokes, "styleIdForStrokeFill"),
    ] {
        let Some(p) = paints.last() else { continue };
        let accent = is_accent(p, &style_name(style_key));
        for geo in n.arr(geo_key) {
            if let Some(blob) = geo.u("commandsBlob") {
                out.push(Shape {
                    accent,
                    d: doc.path(blob, &m, 1.0, 0.0, 0.0, 1.0),
                    evenodd: geo.s("windingRule") == Some("ODD"),
                    opacity: alpha(p),
                });
            }
        }
    }
    if n.s("type") == Some("BOOLEAN_OPERATION") {
        return;
    }
    for &k in doc.children(n) {
        collect(doc, doc.nodes[k], &m, op, out);
    }
}

fn svg(w: f64, h: f64, shapes: &[&Shape]) -> String {
    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {0} {1}" width="{0}" height="{1}">"#,
        g(w),
        g(h)
    );
    for sh in shapes {
        let _ = write!(s, r##"<path d="{}" fill="#FFFFFF""##, sh.d);
        if sh.opacity < 0.999 {
            let _ = write!(s, r#" fill-opacity="{:.3}""#, sh.opacity);
        }
        let _ = write!(
            s,
            r#" fill-rule="{}"/>"#,
            if sh.evenodd { "evenodd" } else { "nonzero" }
        );
    }
    s.push_str("</svg>\n");
    s
}

/// Read the `.fig` (zip) and return its decoded canvas.
pub fn load_fig(fig: &Path) -> Result<Value, String> {
    let file = std::fs::File::open(fig).map_err(|e| format!("{}: {e}", fig.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut canvas = Vec::new();
    zip.by_name("canvas.fig")
        .map_err(|e| e.to_string())?
        .read_to_end(&mut canvas)
        .map_err(|e| e.to_string())?;
    decode_canvas(&canvas)
}

/// Write every icon variant; returns (files, icons).
pub fn extract_icons(msg: &Value, out_dir: &Path) -> Result<(usize, usize), String> {
    let doc = Doc::new(msg);
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    let mut files = 0;
    for n in &doc.nodes {
        if n.s("type") != Some("SYMBOL") {
            continue;
        }
        let Some(name) = n.s("name").filter(|s| s.starts_with("name=")) else {
            continue;
        };
        let props: HashMap<&str, &str> = name
            .split(", ")
            .filter_map(|kv| kv.split_once('='))
            .collect();
        let mut stem = props
            .get("name")
            .copied()
            .unwrap_or("icon")
            .replace(' ', "-");
        for key in ["direction", "state", "value", "EQ"] {
            if let Some(v) = props.get(key).filter(|v| **v != "default") {
                stem.push('-');
                stem.push_str(&v.replace(' ', "-"));
            }
        }
        if props.get("two-tone") == Some(&"yes") {
            stem.push_str("-2t");
        }
        if !seen.insert(stem.clone()) {
            continue;
        }
        let mut shapes = Vec::new();
        for &k in doc.children(n) {
            collect(&doc, doc.nodes[k], &IDENTITY, 1.0, &mut shapes);
        }
        let size = n.get("size");
        let (w, h) = (
            size.and_then(|s| s.f("x")).unwrap_or(24.0),
            size.and_then(|s| s.f("y")).unwrap_or(24.0),
        );
        for (accent, suffix) in [(false, ""), (true, ".accent")] {
            let layer: Vec<&Shape> = shapes
                .iter()
                .filter(|s| s.accent == accent && !s.d.is_empty())
                .collect();
            if layer.is_empty() {
                continue;
            }
            std::fs::write(
                out_dir.join(format!("{stem}{suffix}.svg")),
                svg(w, h, &layer),
            )
            .map_err(|e| e.to_string())?;
            files += 1;
        }
    }
    Ok((files, seen.len()))
}
