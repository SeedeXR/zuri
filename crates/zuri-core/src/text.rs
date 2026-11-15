//! Text layout: glyph outlines from embedded TTF/OTF fonts, laid out as one text, with
//! one outline per stretch of it in one style.
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

/// A stretch of text in one font, size and weight.
pub struct Span<'a> {
    pub font: &'a [u8],
    pub text: &'a str,
    pub size: f32,
    pub weight: f32,
}

/// Lay the spans out one after another, as one text with its top-left at the origin, and
/// return each span's outline. `width` > 0 wraps and is the box that `align` (0 left,
/// 1 center, 2 right) works within; with `width` 0 lines align on x = 0. A line is as
/// tall as the largest size on it (times `line_height`).
pub fn layout(spans: &[Span], width: f32, align: u32, line_height: f32) -> Vec<Path> {
    let mut out = vec![Path::new(); spans.len()];
    // Per span: its font at its weight, and the scale from font units to its size.
    let faces: Vec<Option<(Face, f32)>> = spans
        .iter()
        .map(|span| {
            let mut face = Face::parse(span.font, 0).ok()?;
            // Variable fonts interpolate their outlines to the requested weight; static fonts ignore this.
            face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), span.weight);
            let scale = span.size / face.units_per_em() as f32;
            Some((face, scale))
        })
        .collect();
    // Every character of the text: (span, character, glyph, advance).
    let mut glyphs = Vec::new();
    for (i, span) in spans.iter().enumerate() {
        for ch in span.text.chars() {
            let glyph = faces[i].as_ref().and_then(|(face, _)| face.glyph_index(ch));
            let advance = faces[i].as_ref().map_or(0.0, |(face, scale)| glyph.and_then(|g| face.glyph_hor_advance(g)).unwrap_or(0) as f32 * scale);
            glyphs.push((i, ch, glyph, advance));
        }
    }
    let measure = |line: &[usize]| line.iter().map(|g| glyphs[*g].3).sum::<f32>();

    // Lines, as the glyphs on them. A new line starts at a newline, and (when wrapping)
    // before a word that does not fit; the space before that word is dropped.
    let (mut lines, mut word, mut gap) = (vec![Vec::new()], Vec::new(), None);
    for i in 0..=glyphs.len() {
        let ch = glyphs.get(i).map(|g| g.1);
        if !matches!(ch, None | Some('\n')) && (width <= 0.0 || ch != Some(' ')) {
            word.push(i);
            continue;
        }
        // A word ended (without wrapping, a paragraph is one word): it goes on this line if
        // it fits, with the space before it.
        let space = gap.map_or(0.0, |g: usize| glyphs[g].3);
        if lines.last().is_some_and(|line| !line.is_empty() && measure(line) + space + measure(&word) > width) {
            lines.push(Vec::new());
        }
        let line = lines.last_mut().unwrap();
        if !line.is_empty() {
            line.extend(gap);
        }
        line.append(&mut word);
        gap = (ch == Some(' ')).then_some(i);
        if ch == Some('\n') {
            lines.push(Vec::new());
        }
    }

    // An empty line is as tall as the text before it. `rows` is the sizes of the lines
    // above, added up exactly (f64), so that line n of a text in one size sits at
    // n * size * line_height to the last bit, as it always has.
    let (mut rows, mut tall) = (0.0f64, 0);
    for line in &lines {
        let spans_here = || line.iter().map(|g| glyphs[*g].0);
        tall = spans_here().max_by(|a, b| spans[*a].size.total_cmp(&spans[*b].size)).unwrap_or(tall);
        let ascent = spans_here().filter_map(|i| faces[i].as_ref()).map(|(face, scale)| face.ascender() as f32 * scale).fold(0.0, f32::max);
        let free = width.max(0.0) - measure(line);
        let mut x = match align {
            1 => free * 0.5,
            2 => free,
            _ => 0.0,
        };
        for &(i, _, glyph, advance) in line.iter().map(|g| &glyphs[*g]) {
            if let (Some(g), Some((face, scale))) = (glyph, &faces[i]) {
                let mut b = Builder { out: &mut out[i], scale: *scale, origin: pt(x, ascent + rows as f32 * line_height), cur: P::default() };
                face.outline_glyph(g, &mut b);
            }
            x += advance;
        }
        rows += spans.get(tall).map_or(0.0, |span| span.size as f64);
    }
    out
}
