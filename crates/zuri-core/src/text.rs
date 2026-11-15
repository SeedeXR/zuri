//! Text layout: glyph outlines from an embedded TTF/OTF font, laid out into one path.
//!
// ponytail: one glyph per character with horizontal advances and greedy word wrap. No
// kerning, ligatures, bidi or complex-script shaping. Upgrade path: shape runs with
// harfrust/rustybuzz and feed the positioned glyphs through the same outline builder.

use crate::geom::{pt, El, Path, P};
use ttf_parser::{Face, OutlineBuilder};

struct Builder<'a> {
    out: &'a mut Path,
    scale: f32,
    origin: P,
    cur: P,
}

impl Builder<'_> {
    fn map(&self, x: f32, y: f32) -> P {
        pt(self.origin.x + x * self.scale, self.origin.y - y * self.scale)
    }
}

impl OutlineBuilder for Builder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cur = self.map(x, y);
        self.out.push(El::M(self.cur));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cur = self.map(x, y);
        self.out.push(El::L(self.cur));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (q, end) = (self.map(x1, y1), self.map(x, y));
        let third = |a: P, b: P| pt(a.x + (b.x - a.x) * 2.0 / 3.0, a.y + (b.y - a.y) * 2.0 / 3.0);
        self.out.push(El::C(third(self.cur, q), third(end, q), end));
        self.cur = end;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.cur = self.map(x, y);
        self.out.push(El::C(self.map(x1, y1), self.map(x2, y2), self.cur));
    }
    fn close(&mut self) {
        self.out.push(El::Z);
    }
}

/// Lay `text` out with its top-left at the origin. `width` > 0 wraps and is the box that
/// `align` (0 left, 1 center, 2 right) works within; with `width` 0 lines align on x = 0.
pub fn layout(font: &[u8], text: &str, size: f32, width: f32, align: u32, line_height: f32, weight: f32) -> Path {
    let mut out = Path::new();
    let Ok(mut face) = Face::parse(font, 0) else { return out };
    // Variable fonts interpolate their outlines to the requested weight; static fonts ignore this.
    face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), weight);
    let scale = size / face.units_per_em() as f32;
    let advance = |ch: char| face.glyph_index(ch).and_then(|g| face.glyph_hor_advance(g)).unwrap_or(0) as f32 * scale;
    let measure = |s: &str| s.chars().map(advance).sum::<f32>();

    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        if width <= 0.0 {
            lines.push(para.to_string());
            continue;
        }
        let mut line = String::new();
        for word in para.split(' ') {
            if !line.is_empty() && measure(&line) + advance(' ') + measure(word) > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }

    let ascent = face.ascender() as f32 * scale;
    for (i, line) in lines.iter().enumerate() {
        let free = width.max(0.0) - measure(line);
        let mut x = match align {
            1 => free * 0.5,
            2 => free,
            _ => 0.0,
        };
        let y = ascent + i as f32 * size * line_height;
        for ch in line.chars() {
            if let Some(g) = face.glyph_index(ch) {
                let mut b = Builder { out: &mut out, scale, origin: pt(x, y), cur: P::default() };
                face.outline_glyph(g, &mut b);
            }
            x += advance(ch);
        }
    }
    out
}
