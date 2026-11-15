//! Font subsetting at build time: a `.zur` carries only the glyphs its texts can show.
//!
//! Glyph numbers are kept, so every table that refers to glyphs stays valid as it is. The
//! outlines (`glyf`) and the variation data (`gvar`) of the glyphs that are not needed are
//! emptied, which is where a font's bytes are, and the tables the runtime does not read
//! are left out.
//
// ponytail: TrueType outlines only. A CFF font (most .otf) is embedded whole; subset
// `CFF ` charstrings the same way when someone ships one. Of a collection (.ttc) the
// first font is kept, which is the one the runtime reads.
// ponytail: the result holds what Zuri's text layout reads today, nothing else. `post`,
// hinting (`cvt `, `fpgm`, `prep`, `gasp`), GPOS / GSUB / GDEF and the vertical metrics
// are left out: add a table to TABLES when the runtime starts to read it (they stay
// valid, because glyph numbers do not change).
// ponytail: a glyph that is not kept loses its variation data too. In a variable font
// with no `HVAR` table its advance then stays at the default weight's, so run-time text
// with characters that were not kept (they draw nothing) can be spaced slightly off.

use ttf_parser::{Face, Tag};

/// What the runtime reads, plus `name` (it carries the font's copyright and license).
const TABLES: [&[u8; 4]; 16] =
    [b"GPOS", b"HVAR", b"MVAR", b"OS/2", b"avar", b"cmap", b"fvar", b"glyf", b"gvar", b"head", b"hhea", b"hmtx", b"kern", b"loca", b"maxp", b"name"];

fn be16(d: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?) as usize)
}
fn be32(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// `font` with only the glyphs for `chars` (and `.notdef`). None when the font is not one
/// this can subset: the caller then embeds it whole.
pub fn subset(font: &[u8], chars: &str) -> Option<Vec<u8>> {
    let face = Face::parse(font, 0).ok()?;
    let table = |tag: &[u8; 4]| face.raw_face().table(Tag::from_bytes(tag));
    let (glyf, loca) = (table(b"glyf")?, table(b"loca")?);
    let count = face.number_of_glyphs() as usize;
    let long = face.tables().head.index_to_location_format == ttf_parser::head::IndexToLocationFormat::Long;
    // Where glyph `g`'s outline is in `glyf`.
    let outline = |g: usize| {
        let at = |i: usize| if long { be32(loca, i * 4) } else { be16(loca, i * 2).map(|v| v * 2) };
        glyf.get(at(g)?..at(g + 1)?)
    };

    // The glyphs to keep: .notdef, one per character, and the parts composite glyphs are made of.
    let mut keep = vec![false; count];
    let mut todo: Vec<usize> = std::iter::once(0).chain(chars.chars().filter_map(|c| face.glyph_index(c)).map(|g| g.0 as usize)).collect();
    while let Some(g) = todo.pop() {
        if g >= count || std::mem::replace(&mut keep[g], true) {
            continue;
        }
        let data = outline(g)?;
        if data.len() >= 10 && (be16(data, 0)? as i16) < 0 {
            let mut at = 10;
            loop {
                let flags = be16(data, at)?;
                todo.push(be16(data, at + 2)?);
                // Skip the component's arguments (words or bytes) and its scale, if any.
                at += 4 + if flags & 1 != 0 { 4 } else { 2 };
                at += [(0x08, 2), (0x40, 4), (0x80, 8)].iter().find(|(bit, _)| flags & bit != 0).map_or(0, |(_, size)| *size);
                if flags & 0x20 == 0 {
                    break;
                }
            }
        }
    }

    let (mut new_glyf, mut new_loca) = (Vec::new(), Vec::new());
    for g in 0..=count {
        match long {
            true => new_loca.extend((new_glyf.len() as u32).to_be_bytes()),
            false => new_loca.extend(((new_glyf.len() / 2) as u16).to_be_bytes()),
        }
        if keep.get(g) == Some(&true) {
            new_glyf.extend(outline(g)?);
            new_glyf.resize(new_glyf.len().next_multiple_of(2), 0); // short offsets count in pairs
        }
    }
    let new_gvar = match table(b"gvar") {
        Some(gvar) => Some(subset_gvar(gvar, &keep)?),
        None => None,
    };

    let new_kern = kern_table(&face, &keep);
    // The font's own kerning tables instead, when that is the smaller way to keep it (many
    // glyphs kept, kerned by class).
    let own = table(b"GPOS").filter(|gpos| new_kern.as_ref().is_some_and(|kern| kern.len() > gpos.len()));

    let tables: Vec<(&[u8; 4], &[u8])> = TABLES
        .iter()
        .filter_map(|tag| {
            let data = match *tag {
                b"glyf" => &new_glyf[..],
                b"loca" => &new_loca[..],
                b"gvar" => new_gvar.as_deref()?,
                b"GPOS" => own?,
                b"kern" if own.is_some() => table(tag)?,
                b"kern" => new_kern.as_deref()?,
                _ => table(tag)?,
            };
            Some((*tag, data))
        })
        .collect();
    Some(assemble(&tables))
}

/// A `kern` table of the kerning between the kept glyphs, as the runtime reads it from
/// the font (`zuri_core::text::kerning`). None when no two of them are kerned.
// ponytail: every pair of kept glyphs is asked for, which is slow for thousands of them
// (a CJK font listed whole); walk the font's own pair tables if a build ever waits on it.
fn kern_table(face: &Face, keep: &[bool]) -> Option<Vec<u8>> {
    let kern = zuri_core::text::kerning(face);
    let kept: Vec<u16> = (0..keep.len()).filter(|g| keep[*g]).map(|g| g as u16).collect();
    // In order of (left, right), which is the order the table is searched in.
    let mut pairs = Vec::new();
    for &left in &kept {
        for &right in &kept {
            let by = kern(ttf_parser::GlyphId(left), ttf_parser::GlyphId(right));
            if by != 0 {
                pairs.push([left, right, by.clamp(i16::MIN.into(), i16::MAX.into()) as u16]);
            }
        }
    }
    // A subtable says its length in 16 bits: 10,920 pairs fit.
    (!pairs.is_empty()).then(|| kern_bytes(&pairs, 10_920))
}

/// The pairs (left, right, by), in order, as a `kern` table of subtables of at most `most`.
fn kern_bytes(pairs: &[[u16; 3]], most: usize) -> Vec<u8> {
    let subtables = pairs.chunks(most);
    let mut out = vec![0, 0];
    out.extend((subtables.len() as u16).to_be_bytes());
    for pairs in subtables {
        let n = pairs.len() as u16;
        let log = n.ilog2() as u16;
        // Version, length, format 0 with horizontal kerning, the search header, the pairs.
        for v in [0, 14 + 6 * n, 1, n, 6 << log, log, 6 * n - (6 << log)].iter().chain(pairs.iter().flatten()) {
            out.extend(v.to_be_bytes());
        }
    }
    out
}

/// `gvar` with the variation data of the glyphs that are not kept left out.
fn subset_gvar(gvar: &[u8], keep: &[bool]) -> Option<Vec<u8>> {
    let (axes, tuples, tuples_at, count, flags, data_at) = (be16(gvar, 4)?, be16(gvar, 6)?, be32(gvar, 8)?, be16(gvar, 12)?, be16(gvar, 14)?, be32(gvar, 16)?);
    let at = |i: usize| if flags & 1 != 0 { be32(gvar, 20 + i * 4) } else { be16(gvar, 20 + i * 2).map(|v| v * 2) };
    let shared = gvar.get(tuples_at..tuples_at + tuples * axes * 2)?;
    let (mut offsets, mut data) = (Vec::new(), Vec::<u8>::new());
    for g in 0..=count {
        offsets.extend((data.len() as u32).to_be_bytes());
        if g < count && keep.get(g) == Some(&true) {
            data.extend(gvar.get(data_at + at(g)?..data_at + at(g + 1)?)?);
        }
    }
    // Header, long offsets, the shared tuples, the data.
    let mut out = gvar.get(..20)?.to_vec();
    let shared_at = 20 + offsets.len();
    out[8..12].copy_from_slice(&(shared_at as u32).to_be_bytes());
    out[14..16].copy_from_slice(&((flags | 1) as u16).to_be_bytes());
    out[16..20].copy_from_slice(&((shared_at + shared.len()) as u32).to_be_bytes());
    out.extend(offsets);
    out.extend(shared);
    out.extend(data);
    Some(out)
}

/// A font file from its tables (which must be in tag order), with valid checksums.
fn assemble(tables: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let checksum = |d: &[u8]| d.chunks(4).fold(0u32, |sum, c| sum.wrapping_add(c.iter().enumerate().fold(0, |w, (i, b)| w | (*b as u32) << (24 - 8 * i))));
    let n = tables.len() as u16;
    let log = 15 - n.leading_zeros() as u16; // floor(log2(n)); a font has at least one table
    let mut out = vec![0, 1, 0, 0]; // a font with TrueType outlines
    for v in [n, 16 << log, log, n * 16 - (16 << log)] {
        out.extend(v.to_be_bytes());
    }
    let mut at = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_at = None;
    for (tag, data) in tables {
        let mut data = data.to_vec();
        if *tag == b"head" && data.len() >= 12 {
            data[8..12].fill(0); // the file's own checksum adjustment, filled in below
            head_at = Some(at);
        }
        out.extend(*tag);
        for v in [checksum(&data), at as u32, data.len() as u32] {
            out.extend(v.to_be_bytes());
        }
        data.resize(data.len().next_multiple_of(4), 0);
        at += data.len();
        body.extend(data);
    }
    out.extend(body);
    if let Some(head_at) = head_at {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_kern_table_in_several_subtables_reads_back() {
        // 25 pairs, 10 to a subtable: the lengths and headers of all three have to be right.
        let pairs: Vec<[u16; 3]> = (0..25u16).map(|i| [3 + i / 5, 40 + i % 5, (-(i as i16) - 1) as u16]).collect();
        let bytes = super::kern_bytes(&pairs, 10);
        let table = ttf_parser::kern::Table::parse(&bytes).unwrap();
        assert_eq!(table.subtables.into_iter().count(), 3);
        let read = |left: u16, right: u16| {
            table.subtables.into_iter().filter_map(|sub| sub.glyphs_kerning(ttf_parser::GlyphId(left), ttf_parser::GlyphId(right))).sum::<i16>()
        };
        for [left, right, by] in &pairs {
            assert_eq!(read(*left, *right), *by as i16);
        }
        assert_eq!((read(3, 45), read(2, 40)), (0, 0));
    }
}
