//! `.zur` binary container.
//!
//! ```text
//! "ZURI"                      4-byte fingerprint
//! varuint major, minor, id    readers reject a different major, accept any minor
//! ToC: varuint key ... 0      every property key used in this file
//!      2 bits per key         backing type, packed 4 per byte, low bits first
//! objects until EOF:
//!      varuint type_key
//!      (varuint prop_key, value)* 0
//! ```
//! Backing types: 0 = varuint, 1 = length-prefixed bytes/UTF-8, 2 = f32 LE, 3 = u32 LE color.
//! Because the ToC names the backing type of every key, a reader can skip properties and
//! whole objects it does not know. That is the forward-compatibility contract.
//!
//! Hierarchy is not nesting: property key 1 (`parent`) holds the file index of the parent
//! object, which must precede the child. Objects without it are roots.

use crate::{Error, Result};

pub const MAGIC: &[u8; 4] = b"ZURI";
pub const MAJOR: u64 = 1;
pub const MINOR: u64 = 0;
/// Property key of the parent index. Reserved in every schema version.
pub const PARENT: u16 = 1;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Uint(u64),
    Bytes(Vec<u8>),
    Float(f32),
    Color(u32),
}

impl Value {
    pub fn backing(&self) -> u8 {
        match self {
            Value::Uint(_) => 0,
            Value::Bytes(_) => 1,
            Value::Float(_) => 2,
            Value::Color(_) => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RawObject {
    pub type_key: u16,
    pub props: Vec<(u16, Value)>,
}

impl RawObject {
    pub fn get(&self, key: u16) -> Option<&Value> {
        self.props.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    pub fn uint(&self, key: u16) -> Option<u64> {
        match self.get(key) {
            Some(Value::Uint(v)) => Some(*v),
            _ => None,
        }
    }
    pub fn float(&self, key: u16) -> Option<f32> {
        match self.get(key) {
            Some(Value::Float(v)) => Some(*v),
            _ => None,
        }
    }
    pub fn color(&self, key: u16) -> Option<u32> {
        match self.get(key) {
            Some(Value::Color(v)) => Some(*v),
            _ => None,
        }
    }
    pub fn bytes(&self, key: u16) -> Option<&[u8]> {
        match self.get(key) {
            Some(Value::Bytes(v)) => Some(v),
            _ => None,
        }
    }
    pub fn str(&self, key: u16) -> Option<&str> {
        self.bytes(key).and_then(|b| std::str::from_utf8(b).ok())
    }
    pub fn parent(&self) -> Option<usize> {
        self.uint(PARENT).map(|v| v as usize)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn varuint(&mut self) -> Result<u64> {
        let mut out = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.b.get(self.pos).ok_or_else(|| Error("truncated file".into()))?;
            self.pos += 1;
            out |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(out);
            }
        }
        bail!("varuint longer than 10 bytes at offset {}", self.pos)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.b.len());
        let end = end.ok_or_else(|| Error("truncated file".into()))?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn key(&mut self) -> Result<u16> {
        let k = self.varuint()?;
        u16::try_from(k).map_err(|_| Error(format!("key {k} out of range")))
    }
}

/// Parse a `.zur` file. Never panics on malformed input.
pub fn read(bytes: &[u8]) -> Result<Vec<RawObject>> {
    let mut r = Reader { b: bytes, pos: 0 };
    if r.take(4).ok() != Some(MAGIC) {
        bail!("not a .zur file (missing ZURI fingerprint)");
    }
    let major = r.varuint()?;
    if major != MAJOR {
        bail!("unsupported .zur major version {major} (this runtime reads major {MAJOR})");
    }
    let _minor = r.varuint()?;
    let _file_id = r.varuint()?;

    let mut keys = Vec::new();
    loop {
        let k = r.key()?;
        if k == 0 {
            break;
        }
        keys.push(k);
    }
    let packed = r.take(keys.len().div_ceil(4))?;
    let mut toc = vec![0xffu8; keys.iter().copied().max().map_or(0, |m| m as usize + 1)];
    for (i, k) in keys.iter().enumerate() {
        toc[*k as usize] = (packed[i / 4] >> ((i % 4) * 2)) & 3;
    }

    let mut objects: Vec<RawObject> = Vec::new();
    while r.pos < bytes.len() {
        let type_key = r.key()?;
        if type_key == 0 {
            bail!("object {} has type key 0", objects.len());
        }
        let mut props = Vec::new();
        loop {
            let k = r.key()?;
            if k == 0 {
                break;
            }
            let value = match toc.get(k as usize) {
                Some(0) => Value::Uint(r.varuint()?),
                Some(1) => {
                    let n = r.varuint()?;
                    let n = usize::try_from(n).map_err(|_| Error("truncated file".into()))?;
                    Value::Bytes(r.take(n)?.to_vec())
                }
                Some(2) => Value::Float(f32::from_le_bytes(r.take(4)?.try_into().unwrap())),
                Some(3) => Value::Color(u32::from_le_bytes(r.take(4)?.try_into().unwrap())),
                _ => bail!("property key {k} is not in the table of contents"),
            };
            props.push((k, value));
        }
        let obj = RawObject { type_key, props };
        if let Some(p) = obj.parent() {
            if p >= objects.len() {
                bail!("object {} names parent {p}, which does not precede it", objects.len());
            }
        }
        objects.push(obj);
    }
    Ok(objects)
}

/// Deepest allowed nesting; keeps recursive traversals safe on hostile files.
const MAX_DEPTH: u32 = 256;

/// Children and root (top-level ancestor) of every object, by file index.
pub fn tree(objects: &[RawObject]) -> Result<(Vec<Vec<u32>>, Vec<u32>)> {
    let n = objects.len();
    let (mut children, mut root, mut depth) = (vec![Vec::new(); n], vec![0u32; n], vec![0u32; n]);
    for (i, o) in objects.iter().enumerate() {
        match o.parent() {
            Some(par) if par < i => {
                children[par].push(i as u32);
                root[i] = root[par];
                depth[i] = depth[par] + 1;
                if depth[i] > MAX_DEPTH {
                    bail!("object {i} is nested deeper than {MAX_DEPTH} levels");
                }
            }
            Some(par) => bail!("object {i} names parent {par}, which does not precede it"),
            None => root[i] = i as u32,
        }
    }
    Ok((children, root))
}

fn put_varuint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Serialize objects. Output is a pure function of the input (deterministic bytes).
pub fn write(objects: &[RawObject], file_id: u64) -> Result<Vec<u8>> {
    let mut toc = std::collections::BTreeMap::new();
    for o in objects {
        for (k, v) in &o.props {
            if *k == 0 {
                bail!("property key 0 is reserved");
            }
            if *toc.entry(*k).or_insert(v.backing()) != v.backing() {
                bail!("property key {k} is written with two different backing types");
            }
        }
    }
    let mut out = MAGIC.to_vec();
    put_varuint(&mut out, MAJOR);
    put_varuint(&mut out, MINOR);
    put_varuint(&mut out, file_id);
    for k in toc.keys() {
        put_varuint(&mut out, *k as u64);
    }
    out.push(0);
    let mut packed = vec![0u8; toc.len().div_ceil(4)];
    for (i, b) in toc.values().enumerate() {
        packed[i / 4] |= b << ((i % 4) * 2);
    }
    out.extend_from_slice(&packed);
    for o in objects {
        if o.type_key == 0 {
            bail!("type key 0 is reserved");
        }
        put_varuint(&mut out, o.type_key as u64);
        for (k, v) in &o.props {
            put_varuint(&mut out, *k as u64);
            match v {
                Value::Uint(u) => put_varuint(&mut out, *u),
                Value::Bytes(b) => {
                    put_varuint(&mut out, b.len() as u64);
                    out.extend_from_slice(b);
                }
                Value::Float(f) => out.extend_from_slice(&f.to_le_bytes()),
                Value::Color(c) => out.extend_from_slice(&c.to_le_bytes()),
            }
        }
        out.push(0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<RawObject> {
        vec![
            RawObject { type_key: 1, props: vec![(2, Value::Bytes(b"Main".to_vec())), (9, Value::Float(400.0))] },
            RawObject { type_key: 3, props: vec![(PARENT, Value::Uint(0)), (15, Value::Color(0xff3366cc)), (300, Value::Uint(70000))] },
        ]
    }

    #[test]
    fn roundtrip_is_exact_and_deterministic() {
        let bytes = write(&sample(), 7).unwrap();
        assert_eq!(read(&bytes).unwrap(), sample());
        assert_eq!(write(&read(&bytes).unwrap(), 7).unwrap(), bytes);
    }

    #[test]
    fn rejects_bad_input_without_panicking() {
        assert!(read(b"RIVE").is_err());
        assert!(read(b"").is_err());
        let good = write(&sample(), 0).unwrap();
        // Every truncation either parses a prefix of the objects or errors; none may panic.
        for n in 0..good.len() {
            let _ = read(&good[..n]);
        }
        // Flipping any single byte must never panic either.
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xff;
            let _ = read(&bad);
        }
        let mut future = good.clone();
        future[4] = 2; // major
        assert!(read(&future).unwrap_err().0.contains("major"));
    }

    #[test]
    fn child_must_follow_parent() {
        let objs = vec![RawObject { type_key: 1, props: vec![(PARENT, Value::Uint(0))] }];
        assert!(read(&write(&objs, 0).unwrap()).is_err());
    }
}
