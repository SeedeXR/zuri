//! The JSON source twin of `.zur`: the format people and agents write.
//!
//! ```json
//! { "zuri": 1, "children": [
//!   { "type": "Artboard", "name": "Main", "width": 400, "height": 300, "children": [
//!     { "type": "Shape", "name": "ball", "x": 200, "y": 150, "children": [
//!       { "type": "Ellipse", "width": 80, "height": 80 },
//!       { "type": "Fill", "color": "#ff5533" } ] } ] } ] }
//! ```
//! One rule covers everything: an object is `"type"` + properties from the schema +
//! `"children"`. References to other objects are written as the target's `"name"`.
//! Compiling is deterministic: the same JSON always yields the same bytes.

use crate::format::{RawObject, Value, PARENT};
use crate::schema::{self, p, t, Kind, PropDef, TypeDef};
use crate::{Error, Result};
use serde_json::Value as J;
use std::collections::{HashMap, HashSet};

/// Resolves `"@path"` byte properties. The CLI reads files; the browser build refuses.
pub type Loader<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>>;

const GLOBAL: u32 = u32::MAX;

struct Compiler<'a> {
    out: Vec<RawObject>,
    root_of: Vec<u32>,
    /// (object, property, target name, JSON path)
    refs: Vec<(usize, u16, String, String)>,
    /// (scope root, reference class, name) -> object index, or None when ambiguous.
    names: HashMap<(u32, u8, String), Option<u32>>,
    /// (scope root, type, name) of the objects a host addresses by name.
    host_names: HashSet<(u32, u16, String)>,
    load: Loader<'a>,
}

fn names_of(types: &[u16]) -> String {
    types.iter().filter_map(|k| schema::type_def(*k)).map(|t| t.name).collect::<Vec<_>>().join(", ")
}
fn props_of(ty: &TypeDef) -> String {
    ty.props.iter().flat_map(|g| g.iter()).filter_map(|k| schema::prop(*k)).map(|p| p.name).collect::<Vec<_>>().join(", ")
}

/// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` -> `0xAARRGGBB`.
pub fn parse_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    let wide = |n: u32| (n & 0xf) * 0x11;
    match hex.len() {
        3 => Some(0xff00_0000 | wide(v >> 8) << 16 | wide(v >> 4) << 8 | wide(v)),
        4 => Some(wide(v) << 24 | wide(v >> 12) << 16 | wide(v >> 8) << 8 | wide(v >> 4)),
        6 => Some(0xff00_0000 | v),
        8 => Some((v & 0xff) << 24 | v >> 8),
        _ => None,
    }
}

/// `0xAARRGGBB` -> `#rrggbb`, or `#rrggbbaa` when not opaque.
pub fn format_color(argb: u32) -> String {
    let rgb = argb & 0x00ff_ffff;
    match argb >> 24 {
        0xff => format!("#{rgb:06x}"),
        a => format!("#{rgb:06x}{a:02x}"),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { B64[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for b in s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=') {
        acc = acc << 6 | B64.iter().position(|c| *c == b)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

impl Compiler<'_> {
    fn walk(&mut self, node: &J, parent: Option<(usize, &TypeDef)>, path: &str) -> Result<()> {
        let Some(obj) = node.as_object() else { bail!("{path}: expected an object") };
        let Some(ty_name) = obj.get("type").and_then(J::as_str) else { bail!("{path}: missing \"type\"") };
        let Some(ty) = schema::type_by_name(ty_name) else {
            bail!("{path}: unknown type \"{ty_name}\". Types: {}", schema::TYPES.iter().map(|t| t.name).collect::<Vec<_>>().join(", "))
        };
        match parent {
            None if !ty.parents.is_empty() => bail!("{path}: {ty_name} cannot be at the top level; it goes inside: {}", names_of(ty.parents)),
            Some((_, pt)) if ty.parents.is_empty() => bail!("{path}: {ty_name} is a top-level object and cannot be inside {}", pt.name),
            Some((_, pt)) if !ty.parents.contains(&pt.key) => bail!("{path}: {ty_name} cannot be inside {}; it goes inside: {}", pt.name, names_of(ty.parents)),
            _ => {}
        }
        let idx = self.out.len();
        let root = parent.map_or(idx as u32, |(pi, _)| self.root_of[pi]);
        let mut props = Vec::new();
        if let Some((pi, _)) = parent {
            props.push((PARENT, Value::Uint(pi as u64)));
        }
        for (k, v) in obj {
            if k == "type" || k == "children" {
                continue;
            }
            let here = format!("{path}.{k}");
            // `"value"` holds whatever the Key / Property carries: number, #color or string.
            let sugar = k == "value" && matches!(ty.key, t::KEY | t::PROPERTY_NODE);
            let def: &PropDef = match v.as_str() {
                Some(s) if sugar => schema::prop(if s.starts_with('#') { p::COLOR_VALUE } else { p::TEXT_VALUE }).unwrap(),
                _ => match schema::prop_by_name(k).filter(|d| d.key != PARENT && ty.has(d.key)) {
                    Some(d) => d,
                    None => bail!("{here}: {ty_name} has no property \"{k}\". Its properties: {}", props_of(ty)),
                },
            };
            let wrong = |want: &str| Error(format!("{here}: expected {want}, got {v}"));
            let value = match def.kind {
                Kind::Float => match v {
                    J::Bool(b) => Value::Float(*b as u8 as f32),
                    _ => Value::Float(v.as_f64().map(|f| f as f32).filter(|f| f.is_finite()).ok_or_else(|| wrong("a number"))?),
                },
                Kind::Uint => Value::Uint(v.as_u64().ok_or_else(|| wrong("a whole number >= 0"))?),
                Kind::Bool => match v {
                    J::Bool(b) => Value::Uint(*b as u64),
                    _ => Value::Uint((v.as_f64().ok_or_else(|| wrong("true or false"))? != 0.0) as u64),
                },
                Kind::Color => Value::Color(v.as_str().and_then(parse_color).ok_or_else(|| wrong("a color like \"#rrggbb\" or \"#rrggbbaa\""))?),
                Kind::Str => Value::Bytes(v.as_str().ok_or_else(|| wrong("a string"))?.as_bytes().to_vec()),
                Kind::Bytes => {
                    let s = v.as_str().ok_or_else(|| wrong("base64 or \"@path\""))?;
                    Value::Bytes(match s.strip_prefix('@') {
                        Some(file) => (self.load)(file).map_err(|e| Error(format!("{here}: {e}")))?,
                        None => base64_decode(s).ok_or_else(|| wrong("valid base64"))?,
                    })
                }
                Kind::Ref => {
                    self.refs.push((idx, def.key, v.as_str().ok_or_else(|| wrong("the name of another object"))?.to_string(), here));
                    continue;
                }
                Kind::PropKey => {
                    let name = v.as_str().ok_or_else(|| wrong("a property name"))?;
                    let Some(target) = schema::prop_by_name(name) else { bail!("{here}: \"{name}\" is not a property") };
                    // A Bind writes a property of the object it is in.
                    match parent {
                        Some((_, pt)) if ty.key == t::BIND => {
                            undriven(pt, target, "bind").map_or(Ok(()), |why| Err(Error(format!("{here}: {} {why}", pt.name))))?
                        }
                        _ => {}
                    }
                    Value::Uint(target.key as u64)
                }
                Kind::Enum(values) => {
                    // A cross-fade has no value to swing past: the elastic ones are for Keys.
                    let values = if ty.key == t::TRANSITION && def.key == p::INTERP { &values[..3] } else { values };
                    let pos = v.as_str().and_then(|s| values.iter().position(|x| *x == s));
                    Value::Uint(pos.ok_or_else(|| wrong(&format!("one of: {}", values.join(", "))))? as u64)
                }
            };
            props.push((def.key, value));
        }
        // What shapes the value a Bind gives a property has nothing to shape without one;
        // and a Bind that gives a copy its position gives no property anything.
        if ty.key == t::BIND {
            let has = |key: &str| obj.contains_key(key);
            let idle = ["mul", "add", "min", "max", "round", "direction", "format", "decimals"].into_iter().find(|key| has(key));
            match idle {
                Some(key) if !has("property") => bail!("{path}: \"{key}\" is for a Bind that has a \"property\" to write; this one has none"),
                _ if has("index") && has("property") => {
                    bail!("{path}: a Bind with \"index\" writes a copy's position \"into\" a Property of the nested artboard; it has no \"property\"")
                }
                _ => {}
            }
        }
        if let Some(name) = obj.get("name").and_then(J::as_str) {
            let scope = if parent.is_none() { GLOBAL } else { root };
            // A `/` is what a host's path into a nested artboard is cut at.
            if matches!(ty.key, t::PROPERTY_NODE | t::NESTED_ARTBOARD) && name.contains('/') {
                bail!("{path}: the name \"{name}\" has a \"/\" in it, which is what parts a host's path to a nested artboard's property (\"card/title\"); name it without");
            }
            // In a path a number after a NestedArtboard's name is which copy of a list.
            if ty.key == t::NESTED_ARTBOARD && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
                bail!("{path}: the name \"{name}\" is a number, which in a host's path is the position of a copy in a list (\"rows/2/title\"); give it a name that is not");
            }
            if schema::host_named(ty.key) && !name.is_empty() && !self.host_names.insert((scope, ty.key, name.to_string())) {
                bail!("{path}: another {ty_name} is already named \"{name}\"; the host finds it by name, so the name must be unique");
            }
            for scope in if parent.is_none() { vec![root, GLOBAL] } else { vec![root] } {
                self.names.entry((scope, schema::ref_class(ty.key), name.to_string())).and_modify(|e| *e = None).or_insert(Some(idx as u32));
            }
        }
        self.out.push(RawObject { type_key: ty.key, props });
        self.root_of.push(root);
        match obj.get("children") {
            None => {}
            Some(J::Array(kids)) => {
                for (i, kid) in kids.iter().enumerate() {
                    self.walk(kid, Some((idx, ty)), &format!("{path}.children[{i}]"))?;
                }
            }
            Some(_) => bail!("{path}.children: expected an array"),
        }
        Ok(())
    }
}

/// Compile JSON source into objects (validated against the schema, references resolved).
pub fn compile(src: &str, load: Loader) -> Result<Vec<RawObject>> {
    let doc: J = serde_json::from_str(src).map_err(|e| Error(format!("invalid JSON: {e}")))?;
    if let Some(v) = doc.get("zuri") {
        if v.as_u64() != Some(1) {
            bail!("unsupported source version {v} (this compiler reads \"zuri\": 1)");
        }
    }
    let Some(roots) = doc.get("children").and_then(J::as_array) else { bail!("the document needs a top-level \"children\" array of Artboards and assets") };
    let mut c = Compiler { out: Vec::new(), root_of: Vec::new(), refs: Vec::new(), names: HashMap::new(), host_names: HashSet::new(), load };
    for (i, root) in roots.iter().enumerate() {
        c.walk(root, None, &format!("children[{i}]"))?;
    }
    for (idx, key, name, path) in std::mem::take(&mut c.refs) {
        let class = schema::expected_class(c.out[idx].type_key, key);
        let what = schema::CLASS_NAMES[class as usize];
        let found = c.names.get(&(c.root_of[idx], class, name.clone())).or_else(|| c.names.get(&(GLOBAL, class, name.clone())));
        match found {
            Some(Some(target)) => {
                let (want, got) = (schema::ref_types(c.out[idx].type_key, key), c.out[*target as usize].type_key);
                if !want.is_empty() && !want.contains(&got) {
                    bail!("{path}: \"{name}\" is a {}; this needs: {}", names_of(&[got]), names_of(want));
                }
                // A Track animates a property of its target: one that object has.
                let keyed = c.out[idx].uint(p::PROPERTY).filter(|_| c.out[idx].type_key == t::TRACK).and_then(|k| schema::prop(k as u16));
                if let Some((ty, why)) = keyed.zip(schema::type_def(got)).and_then(|(prop, ty)| Some((ty, undriven(ty, prop, "animate")?))) {
                    bail!("{path}: {} \"{name}\" {why}", ty.name);
                }
                c.out[idx].props.push((key, Value::Uint(*target as u64)))
            }
            Some(None) => bail!("{path}: more than one {what} is named \"{name}\"; names that are referred to must be unique"),
            None => {
                let mut known: Vec<&str> = c
                    .names
                    .keys()
                    .filter(|(scope, cl, _)| (*scope == c.root_of[idx] || *scope == GLOBAL) && *cl == class)
                    .map(|(_, _, n)| n.as_str())
                    .collect();
                known.sort_unstable();
                known.dedup();
                bail!("{path}: no {what} is named \"{name}\" here. Available: {}", if known.is_empty() { "(none)".to_string() } else { known.join(", ") })
            }
        }
    }
    // Canonical property order makes the bytes independent of JSON key order.
    for o in &mut c.out {
        o.props.sort_by_key(|(k, _)| *k);
    }
    Ok(c.out)
}

/// What is wrong with a Bind or a Track driving `prop` of an object of type `ty`, if
/// anything: the object has to have the property, and it has to hold a value (a number,
/// a color, text), not a reference to another object, a property's name or a file.
fn undriven(ty: &TypeDef, prop: &PropDef, verb: &str) -> Option<String> {
    if !ty.has(prop.key) {
        Some(format!("has no property \"{}\" to {verb}. Its properties: {}", prop.name, props_of(ty)))
    } else if matches!(prop.kind, Kind::Ref | Kind::PropKey | Kind::Bytes) {
        Some(format!("cannot {verb} \"{}\": it names another object or holds a file, not a value", prop.name))
    } else {
        None
    }
}

/// Turn objects back into JSON source. Unknown types and properties (from a newer
/// format version) are dropped.
pub fn decompile(objects: &[RawObject]) -> Result<String> {
    let (children, root) = crate::format::tree(objects)?;
    // Every referenced object needs a name that is unique in its scope.
    let mut count: HashMap<(u32, u8, &str), u32> = HashMap::new();
    for (i, o) in objects.iter().enumerate() {
        if let Some(name) = o.str(p::NAME) {
            *count.entry((root[i], schema::ref_class(o.type_key), name)).or_default() += 1;
        }
    }
    let mut label: Vec<Option<String>> = objects.iter().map(|o| o.str(p::NAME).map(String::from)).collect();
    for o in objects {
        for (k, v) in &o.props {
            let (Some(Kind::Ref), Value::Uint(target)) = (schema::prop(*k).map(|d| d.kind), v) else { continue };
            let ti = *target as usize;
            let Some(to) = objects.get(ti) else { bail!("a reference points at object {target}, which does not exist") };
            let unique = to.str(p::NAME).is_some_and(|name| count[&(root[ti], schema::ref_class(to.type_key), name)] == 1);
            if !unique {
                label[ti] = Some(format!("{}{ti}", schema::type_def(to.type_key).map_or("Object", |t| t.name)));
            }
        }
    }

    fn emit(out: &mut String, objects: &[RawObject], children: &[Vec<u32>], label: &[Option<String>], i: usize, indent: usize) {
        let o = &objects[i];
        let Some(ty) = schema::type_def(o.type_key) else { return };
        let pad = "  ".repeat(indent);
        let quote = |s: &str| serde_json::to_string(s).unwrap_or_default();
        out.push_str(&format!("{pad}{{\n{pad}  \"type\": \"{}\"", ty.name));
        if let Some(name) = &label[i] {
            out.push_str(&format!(",\n{pad}  \"name\": {}", quote(name)));
        }
        for (k, v) in &o.props {
            let Some(def) = schema::prop(*k).filter(|d| d.key != PARENT && d.key != p::NAME && ty.has(d.key)) else { continue };
            let text = match (def.kind, v) {
                (Kind::Float, Value::Float(f)) => f.to_string(),
                (Kind::Uint, Value::Uint(u)) => u.to_string(),
                (Kind::Bool, Value::Uint(u)) => (*u != 0).to_string(),
                (Kind::Color, Value::Color(c)) => quote(&format_color(*c)),
                (Kind::Str, Value::Bytes(b)) => quote(&String::from_utf8_lossy(b)),
                (Kind::Bytes, Value::Bytes(b)) => quote(&base64_encode(b)),
                (Kind::Ref, Value::Uint(u)) => quote(label.get(*u as usize).and_then(|l| l.as_deref()).unwrap_or("")),
                (Kind::PropKey, Value::Uint(u)) => quote(schema::prop(*u as u16).map_or("", |d| d.name)),
                (Kind::Enum(values), Value::Uint(u)) => quote(values.get(*u as usize).copied().unwrap_or(values[0])),
                _ => continue,
            };
            out.push_str(&format!(",\n{pad}  \"{}\": {text}", def.name));
        }
        let kids: Vec<usize> = children[i].iter().map(|c| *c as usize).filter(|c| schema::type_def(objects[*c].type_key).is_some()).collect();
        if !kids.is_empty() {
            out.push_str(&format!(",\n{pad}  \"children\": [\n"));
            for (n, kid) in kids.iter().enumerate() {
                emit(out, objects, children, label, *kid, indent + 2);
                out.push_str(if n + 1 < kids.len() { ",\n" } else { "\n" });
            }
            out.push_str(&format!("{pad}  ]"));
        }
        out.push_str(&format!("\n{pad}}}"));
    }

    let mut out = String::from("{\n  \"zuri\": 1,\n  \"children\": [\n");
    let roots: Vec<usize> = (0..objects.len()).filter(|i| objects[*i].parent().is_none() && schema::type_def(objects[*i].type_key).is_some()).collect();
    for (k, i) in roots.iter().enumerate() {
        emit(&mut out, objects, &children, &label, *i, 2);
        out.push_str(if k + 1 < roots.len() { ",\n" } else { "\n" });
    }
    out.push_str("  ]\n}\n");
    Ok(out)
}

/// The schema as JSON: every type, where it may live, and its properties. This is what
/// `zuri schema` prints and what authoring agents should read first.
pub fn schema_json() -> String {
    let types: Vec<J> = schema::TYPES
        .iter()
        .map(|ty| {
            let props: Vec<J> = ty
                .props
                .iter()
                .flat_map(|g| g.iter())
                .filter_map(|k| schema::prop(*k))
                .map(|d| {
                    let (kind, values): (&str, &[&str]) = match d.kind {
                        Kind::Uint => ("integer", &[]),
                        Kind::Bool => ("boolean", &[]),
                        Kind::Float => ("number", &[]),
                        Kind::Color => ("color", &[]),
                        Kind::Str => ("string", &[]),
                        Kind::Bytes => ("bytes", &[]),
                        Kind::Ref => ("reference", &[]),
                        Kind::PropKey => ("property-name", &[]),
                        Kind::Enum(v) => ("enum", v),
                    };
                    let mut j = serde_json::json!({ "name": d.name, "key": d.key, "kind": kind, "doc": d.doc });
                    match d.kind {
                        Kind::Ref => {
                            j["target"] = J::from(schema::CLASS_NAMES[schema::expected_class(ty.key, d.key) as usize]);
                            let types = schema::ref_types(ty.key, d.key);
                            if !types.is_empty() {
                                j["types"] = types.iter().filter_map(|k| schema::type_def(*k)).map(|t| J::from(t.name)).collect();
                            }
                        }
                        Kind::Enum(v) => {
                            j["values"] = values.iter().map(|s| J::from(*s)).collect();
                            j["default"] = J::from(v[d.default as usize]);
                        }
                        Kind::Float | Kind::Uint => j["default"] = serde_json::json!(d.default),
                        Kind::Bool => j["default"] = J::from(d.default != 0.0),
                        _ => {}
                    }
                    j
                })
                .collect();
            serde_json::json!({
                "type": ty.name,
                "doc": ty.doc,
                // What tools need to resolve references the way the compiler does: names are
                // unique per class within an artboard, and a reference names its target's class.
                "class": schema::CLASS_NAMES[schema::ref_class(ty.key) as usize],
                "scene": schema::is_scene(ty.key),
                "parents": ty.parents.iter().filter_map(|k| schema::type_def(*k)).map(|t| t.name).collect::<Vec<_>>(),
                "properties": props,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "zuri": 1,
        "document": "{ \"zuri\": 1, \"children\": [ ...top-level objects... ] }. Every object is { \"type\", ...properties, \"children\": [...] }. A reference is the \"name\" of its target.",
        "types": types,
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;

    fn no_files(_: &str) -> Result<Vec<u8>> {
        Err(Error("no files here".into()))
    }

    const DOC: &str = r##"{ "zuri": 1, "children": [
      { "type": "Artboard", "name": "Main", "width": 200, "height": 100, "children": [
        { "type": "Shape", "name": "ball", "x": 50.5, "children": [
          { "type": "Ellipse", "width": 20, "height": 20 },
          { "type": "Fill", "color": "#f53", "fillRule": "evenodd" } ] },
        { "type": "Animation", "name": "move", "loop": "pingpong", "children": [
          { "type": "Track", "target": "ball", "property": "x", "children": [
            { "type": "Key", "frame": 0, "value": 50.5 },
            { "type": "Key", "frame": 60, "value": 150, "interp": "cubic" } ] } ] } ] } ] }"##;

    #[test]
    fn compile_resolves_names_enums_and_colors() {
        let objs = compile(DOC, &no_files).unwrap();
        assert_eq!(objs.len(), 8);
        assert_eq!(objs[3].color(p::COLOR), Some(0xffff5533));
        assert_eq!(objs[3].uint(p::FILL_RULE), Some(1));
        assert_eq!(objs[5].uint(p::TARGET), Some(1)); // "ball"
        assert_eq!(objs[5].uint(p::PROPERTY), Some(p::X as u64));
        assert_eq!(objs[4].uint(p::LOOP), Some(2));
    }

    #[test]
    fn roundtrip_is_a_fixpoint() {
        let objs = compile(DOC, &no_files).unwrap();
        let bytes = format::write(&objs, 0).unwrap();
        let json = decompile(&format::read(&bytes).unwrap()).unwrap();
        let again = format::write(&compile(&json, &no_files).unwrap(), 0).unwrap();
        assert_eq!(bytes, again, "json -> zur -> json -> zur must reproduce the same bytes");
    }

    #[test]
    fn key_order_does_not_change_the_bytes() {
        let a = r##"{"children":[{"type":"Artboard","width":10,"height":20,"name":"A"}]}"##;
        let b = r##"{"children":[{"name":"A","height":20,"type":"Artboard","width":10}]}"##;
        assert_eq!(compile(a, &no_files).unwrap(), compile(b, &no_files).unwrap());
    }

    #[test]
    fn errors_say_where_and_what() {
        let err = |src: &str| compile(src, &no_files).unwrap_err().0;
        assert!(err("{").contains("invalid JSON"));
        assert!(err(r#"{"children":[{"type":"Shape"}]}"#).contains("cannot be at the top level"));
        assert!(err(r#"{"children":[{"type":"Artboard","children":[{"type":"Ellipse"}]}]}"#).contains("cannot be inside Artboard; it goes inside: Shape"));
        let e = err(r#"{"children":[{"type":"Artboard","colour":"red"}]}"#);
        assert!(e.contains("children[0].colour") && e.contains("no property"), "{e}");
        let e = err(r#"{"children":[{"type":"Artboard","children":[{"type":"Shape","blendMode":"shiny"}]}]}"#);
        assert!(e.contains("one of: normal, multiply"), "{e}");
        let e = err(r#"{"children":[{"type":"Artboard","children":[{"type":"Animation","children":[{"type":"Track","target":"ghost","property":"x"}]}]}]}"#);
        assert!(e.contains("no drawable object is named \"ghost\""), "{e}");
        let e = err(
            r#"{"children":[{"type":"Artboard","children":[{"type":"Node","name":"a"},{"type":"Node","name":"a"},{"type":"Animation","children":[{"type":"Track","target":"a","property":"x"}]}]}]}"#,
        );
        assert!(e.contains("more than one drawable object"), "{e}");
        // A reference must reach the right kind of thing, and what a host plays by name is unique.
        let e = err(r#"{"children":[{"type":"FontAsset","name":"f","bytes":""},{"type":"Artboard","children":[{"type":"Image","asset":"f"}]}]}"#);
        assert!(e.contains("\"f\" is a FontAsset; this needs: ImageAsset"), "{e}");
        let e = err(r#"{"children":[{"type":"Artboard","children":[{"type":"Animation","name":"a"},{"type":"Animation","name":"a"}]}]}"#);
        assert!(e.contains("children[0].children[1]: another Animation is already named \"a\""), "{e}");
        let e = err(r#"{"children":[{"type":"Artboard","name":"a"},{"type":"Artboard","name":"a"}]}"#);
        assert!(e.contains("children[1]: another Artboard is already named \"a\""), "{e}");
        // The same name in another artboard, or on another type, is fine.
        compile(r#"{"children":[{"type":"Artboard","name":"a","children":[{"type":"Animation","name":"a"}]},{"type":"Artboard","children":[{"type":"Animation","name":"a"}]}]}"#, &no_files).unwrap();
    }

    #[test]
    fn colors_and_base64() {
        assert_eq!(parse_color("#fff"), Some(0xffffffff));
        assert_eq!(parse_color("#11223344"), Some(0x44112233));
        assert_eq!(parse_color("#1234"), Some(0x44112233));
        assert_eq!(parse_color("red"), None);
        assert_eq!(format_color(0x44112233), "#11223344");
        assert_eq!(format_color(0xff112233), "#112233");
        for data in [&b""[..], b"a", b"ab", b"abc", b"\x00\xff\x10zuri"] {
            assert_eq!(base64_decode(&base64_encode(data)).as_deref(), Some(data));
        }
        assert_eq!(base64_encode(b"zuri"), "enVyaQ==");
    }

    #[test]
    fn schema_json_lists_every_type() {
        let j: J = serde_json::from_str(&schema_json()).unwrap();
        assert_eq!(j["types"].as_array().unwrap().len(), schema::TYPES.len());
    }
}
