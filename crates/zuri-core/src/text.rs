//! Text layout: glyph outlines from embedded TTF/OTF fonts, laid out as one text, with
//! one outline per stretch of it in one style.
//!
// ponytail: one glyph per character with horizontal advances, pair kerning and greedy
// word wrap. No ligatures, bidi or complex-script shaping. Upgrade path: shape runs with
// harfrust/rustybuzz and feed the positioned glyphs through the same outline builder.

use crate::geom::{pt, El, Mat, Path, P};
use ttf_parser::gpos::{PairAdjustment, PositioningSubtable};
use ttf_parser::opentype_layout::Lookup;
use ttf_parser::{Face, GlyphId, OutlineBuilder, Tag};

/// The font's kerning: a function from two glyphs to how far the second moves when it
/// follows the first (negative: closer), in the font's units. Read from the pair
/// adjustments of the `kern` feature (GPOS) or, in a font without those, its `kern` table.
// ponytail: the `kern` feature of one script's default language (Latin, else the font's
// default script, else its first), and its values at the default weight (a variable
// font's kerning does not follow `weight`). In a legacy `kern` table, "minimum" and
// "override" subtables are read as plain kerning (ttf-parser does not tell them apart).
// Kerning by context and by anchors (marks, cursive scripts) is shaping: see above.
pub fn kerning<'a>(face: &Face<'a>) -> impl Fn(GlyphId, GlyphId) -> i32 + 'a {
    // A font cannot make a pair cost more than this many lookups (or `kern` subtables),
    // of four times as many subtables each: its tables may point at each other in circles.
    const MOST: usize = 64;
    let (kern, mut lookups) = (face.tables().kern, Vec::new());
    if let Some(gpos) = face.tables().gpos {
        let script = [b"latn", b"DFLT"].iter().find_map(|tag| gpos.scripts.find(Tag::from_bytes(tag))).or_else(|| gpos.scripts.get(0));
        let features = script.and_then(|script| script.default_language).into_iter().flat_map(|language| language.feature_indices);
        let kerns = features.filter_map(|i| gpos.features.get(i)).filter(|feature| feature.tag == Tag::from_bytes(b"kern"));
        let mut indices: Vec<u16> = kerns.flat_map(|feature| feature.lookup_indices).take(4 * MOST).collect();
        indices.sort_unstable();
        indices.dedup();
        lookups = indices.into_iter().take(MOST).filter_map(|i| gpos.lookups.get(i)).collect();
    }
    move |left, right| {
        if lookups.is_empty() {
            let plain = kern.into_iter().flat_map(|kern| kern.subtables).take(MOST).filter(|sub| sub.horizontal && !sub.variable && !sub.has_cross_stream);
            return plain.filter_map(|sub| sub.glyphs_kerning(left, right)).map(i32::from).sum();
        }
        // Every lookup has its say; within one, the first subtable that has the pair. A
        // subtable of classes has every pair whose left glyph it covers (as HarfBuzz
        // reads it: PairPosFormat2.hh, read 2026-11-15).
        let found = |sub: PositioningSubtable| match sub {
            PositioningSubtable::Pair(PairAdjustment::Format1 { coverage, sets }) => Some(sets.get(coverage.get(left)?)?.get(right)?.0.x_advance),
            PositioningSubtable::Pair(PairAdjustment::Format2 { coverage, classes, matrix }) => {
                coverage.get(left)?;
                Some(matrix.get((classes.0.get(left), classes.1.get(right)))?.0.x_advance)
            }
            _ => None,
        };
        // By number, so that a subtable that cannot be read does not hide the ones after it.
        let within =
            |lookup: &Lookup| (0..lookup.subtables.len().min(4 * MOST as u16)).filter_map(|i| lookup.subtables.get::<PositioningSubtable>(i)).find_map(found);
        lookups.iter().filter_map(within).map(i32::from).sum()
    }
}

struct Builder<'a> {
    out: &'a mut Path,
    /// From the glyph's units (y up) to where it stands in the text.
    to: Mat,
    cur: P,
}

impl Builder<'_> {
    fn map(&self, x: f32, y: f32) -> P {
        self.to.apply(pt(x, y))
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
///
/// With `along`, the text is set on a line that is not straight: `along(d)` is the point
/// `d` along it and the direction it runs in there, in degrees (None past its ends). A
/// glyph stands on the line from where it starts to where it ends (so it turns smoothly
/// as it moves), and is left out if either is past an end. Lines after the first run
/// beside the line, at the same places along it.
pub fn layout(spans: &[Span], width: f32, align: u32, line_height: f32, along: Option<&dyn Fn(f32) -> Option<(P, f32)>>) -> Vec<Path> {
    let mut out = vec![Path::new(); spans.len()];
    // Per span: its font at its weight, the scale from font units to its size, its kerning.
    let faces: Vec<Option<_>> = spans
        .iter()
        .map(|span| {
            let mut face = Face::parse(span.font, 0).ok()?;
            // Variable fonts interpolate their outlines to the requested weight; static fonts ignore this.
            face.set_variation(ttf_parser::Tag::from_bytes(b"wght"), span.weight);
            let scale = span.size / face.units_per_em() as f32;
            let kern = kerning(&face);
            Some((face, scale, kern))
        })
        .collect();
    // Every character of the text: (span, character, glyph, advance).
    // Every character of the text: (span, character, glyph, advance, kerning before it).
    let mut glyphs: Vec<(usize, char, Option<GlyphId>, f32, f32)> = Vec::new();
    for (i, span) in spans.iter().enumerate() {
        for ch in span.text.chars() {
            let glyph = faces[i].as_ref().and_then(|(face, _, _)| face.glyph_index(ch));
            let advance = faces[i].as_ref().map_or(0.0, |(face, scale, _)| glyph.and_then(|g| face.glyph_hor_advance(g)).unwrap_or(0) as f32 * scale);
            // Kerned against the glyph before it, when that is in the same font at the same
            // size (also across spans).
            let mut tuck = 0.0;
            if let (Some(&(before, _, Some(left), ..)), Some(right), Some((_, scale, kern))) = (glyphs.last(), glyph, &faces[i]) {
                if std::ptr::eq(spans[before].font, span.font) && spans[before].size == span.size {
                    tuck = kern(left, right) as f32 * scale;
                }
            }
            glyphs.push((i, ch, glyph, advance, tuck));
        }
    }
    // The first glyph of a line is not kerned: what it followed is on the line above.
    let measure = |line: &[usize]| line.iter().enumerate().map(|(at, g)| glyphs[*g].3 + if at > 0 { glyphs[*g].4 } else { 0.0 }).sum::<f32>();

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
        let space = gap.map_or(0.0, |g: usize| glyphs[g].3 + glyphs[g].4);
        let wide = measure(&word) + word.first().map_or(0.0, |g| glyphs[*g].4);
        if lines.last().is_some_and(|line| !line.is_empty() && measure(line) + space + wide > width) {
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
        let ascent = spans_here().filter_map(|i| faces[i].as_ref()).map(|(face, scale, _)| face.ascender() as f32 * scale).fold(0.0, f32::max);
        let free = width.max(0.0) - measure(line);
        let mut x = match align {
            1 => free * 0.5,
            2 => free,
            _ => 0.0,
        };
        for (at, &(i, _, glyph, advance, tuck)) in line.iter().map(|g| &glyphs[*g]).enumerate() {
            if at > 0 {
                x += tuck;
            }
            if let (Some(g), Some((face, scale, _))) = (glyph, &faces[i]) {
                let (down, flip) = (rows as f32 * line_height, Mat::trs(0.0, 0.0, 0.0, *scale, -*scale));
                let to = match along.map(|along| (along(x), along(x + advance))) {
                    None => Mat::trs(x, ascent + down, 0.0, *scale, -*scale),
                    Some((Some((a, facing)), Some((b, _)))) => {
                        let degrees = if a == b { facing } else { (b.y - a.y).atan2(b.x - a.x).to_degrees() };
                        Mat::trs(a.x, a.y, degrees, 1.0, 1.0).mul(&Mat { ty: down, ..flip })
                    }
                    Some(_) => {
                        x += advance;
                        continue;
                    }
                };
                let mut b = Builder { out: &mut out[i], to, cur: P::default() };
                face.outline_glyph(g, &mut b);
            }
            x += advance;
        }
        rows += spans.get(tall).map_or(0.0, |span| span.size as f64);
    }
    out
}
