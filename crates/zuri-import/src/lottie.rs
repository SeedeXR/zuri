//! Lottie (the JSON that Bodymovin and others write) to Zuri source: the layers of the
//! composition as Nodes and Shapes, and one Animation with a Track for every property
//! the file keys. What Zuri has nothing for is left out and named in the notes.
//!
//! Read against the Lottie specification (lottie.github.io/lottie-spec, 2026-11-18) and,
//! for what it leaves unsaid (the first layer is the one on top; a parent hands on its
//! transform and not its opacity), lottiefiles.github.io/lottie-docs.

use serde_json::{json, Map, Value as J};
use std::collections::BTreeSet;
use zuri_core::{Error, Result};

type Obj = Map<String, J>;

/// One keyframe: when, the value there, and how the way to the next one goes (the two
/// handles of its curve: out x, out y, in x, in y, each a number for every dimension).
struct Kf {
    t: f64,
    s: Vec<f64>,
    hold: bool,
    ease: Option<[Vec<f64>; 4]>,
}

fn nums(v: &J) -> Vec<f64> {
    match v {
        J::Array(a) => a.iter().filter_map(J::as_f64).collect(),
        _ => v.as_f64().into_iter().collect(),
    }
}

/// To three decimals, which is finer than a pixel shows and keeps the source readable.
fn num(v: f64) -> J {
    // (Nothing added to nothing below nothing is nothing: no "-0.0".)
    json!((v * 1000.0).round() / 1000.0 + 0.0)
}

fn obj(ty: &str) -> Obj {
    Obj::from_iter([("type".to_string(), json!(ty))])
}

/// The keyframes of a property as they are written, whatever their values are. A value
/// that is not keyed has none.
fn frames(prop: &J) -> &[J] {
    prop["k"].as_array().filter(|a| a.first().is_some_and(J::is_object)).map_or(&[], Vec::as_slice)
}

/// The value at keyframe `n`. Files from before 2018 give the last key none: it is
/// where the one before ends, or where that one is when it holds.
fn value(frames: &[J], n: usize) -> &J {
    match (&frames[n]["s"], n.checked_sub(1).map(|before| &frames[before]["e"])) {
        (J::Null, Some(J::Null)) => value(frames, n - 1),
        (J::Null, Some(end)) => end,
        (s, _) => s,
    }
}

/// The keyframes of a property of numbers; one, at no time, for a value that is not keyed.
fn keys(prop: &J) -> Vec<Kf> {
    let frames = frames(prop);
    if frames.is_empty() {
        return vec![Kf { t: 0.0, s: nums(&prop["k"]), hold: false, ease: None }];
    }
    let key = |(n, f): (usize, &J)| {
        let ease = (f["o"].is_object() && f["i"].is_object()).then(|| [&f["o"]["x"], &f["o"]["y"], &f["i"]["x"], &f["i"]["y"]].map(nums));
        Kf { t: f["t"].as_f64().unwrap_or(0.0), s: nums(value(frames, n)), hold: f["h"].as_f64() == Some(1.0), ease }
    };
    frames.iter().enumerate().map(key).collect()
}

/// A Lottie color (0 to 1 a channel) with an opacity of 0 to 1 as its alpha.
fn hex(c: &[f64], opacity: f64) -> J {
    // (Files from before 2017 give 0 to 255.)
    let unit = if c.iter().take(3).any(|v| *v > 1.0) { 1.0 } else { 255.0 };
    let ch = |i: usize| ((c.get(i).copied().unwrap_or(0.0) * unit).clamp(0.0, 255.0) + 0.5) as u32;
    let a = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    json!(zuri_core::json::format_color(a << 24 | ch(0) << 16 | ch(1) << 8 | ch(2)))
}

/// A Lottie bezier as SVG path data: its tangents are from their vertex. `closed`: the
/// shape it is of says so (files from before 2018 do; later ones say it in the bezier).
fn path(b: &J, closed: bool) -> String {
    let points = |key: &str| b[key].as_array().map(|a| a.iter().map(nums).collect::<Vec<_>>()).unwrap_or_default();
    let (v, i, o) = (points("v"), points("i"), points("o"));
    let at = |list: &[Vec<f64>], n: usize, d: usize| list.get(n).and_then(|p| p.get(d)).copied().unwrap_or(0.0);
    let closed = (closed || b["c"] == true || b["c"] == 1) && v.len() > 1;
    // (To three decimals, and a whole number without a point.)
    let n = |v: f64| ((v * 1000.0).round() / 1000.0 + 0.0).to_string();
    let mut d = String::new();
    for k in 0..v.len() + closed as usize {
        let to = k % v.len();
        let [x, y] = [0, 1].map(|d| at(&v, to, d));
        if k == 0 {
            d += &format!("M{} {}", n(x), n(y));
        } else {
            let [ox, oy] = [0, 1].map(|d| at(&v, k - 1, d) + at(&o, k - 1, d));
            let [ix, iy] = [0, 1].map(|d| at(&v, to, d) + at(&i, to, d));
            d += &format!(" C{} {} {} {} {} {}", n(ox), n(oy), n(ix), n(iy), n(x), n(y));
        }
    }
    d + if closed { " Z" } else { "" }
}

/// A Lottie shape with the trim that cuts it: the first one after it in its list, or
/// else the one that cuts the group it is in.
type Cut<'a> = (&'a J, Option<&'a J>);

struct Lottie<'a> {
    assets: &'a [J],
    /// What of the artboard's timeline is played: from 0 to this.
    end: f64,
    /// How many more layers are taken: precompositions in precompositions (or in
    /// themselves) can ask for more than any file means.
    room: u32,
    tracks: Vec<J>,
    notes: BTreeSet<&'static str>,
    named: u32,
}

impl Lottie<'_> {
    /// The name of an object that a Track is to find: it gets one that nothing else has.
    fn name(&mut self, o: &mut Obj, hint: &str) -> String {
        if let Some(name) = o.get("name").and_then(J::as_str) {
            return name.to_string();
        }
        self.named += 1;
        let name = format!("{hint} {}", self.named);
        o.insert("name".into(), json!(name));
        name
    }

    fn keys(&mut self, prop: &J) -> Vec<Kf> {
        if prop["x"].is_string() {
            self.notes.insert("expressions (the keys under them are kept)");
        }
        keys(prop)
    }

    /// A Track of `property` of the object called `target`, a Key for each keyframe:
    /// `value` gives the Key's field and what goes in it; `dim` is which of a keyframe's
    /// curves is its own; `time` is where the composition's frame 0 is on the artboard's
    /// timeline.
    fn track(&mut self, target: &str, property: &str, keys: &[Kf], dim: usize, time: f64, value: &dyn Fn(usize, &Kf) -> (&'static str, J)) {
        let key = |(n, k): (usize, &Kf)| {
            let (field, v) = value(n, k);
            let mut key = obj("Key");
            key.insert("frame".into(), num(k.t + time));
            key.insert(field.into(), v);
            let last = n + 1 == keys.len();
            if k.hold && !last {
                key.insert("interp".into(), json!("hold"));
            } else if let Some(e) = k.ease.as_ref().filter(|_| !last) {
                // (In time a handle stays between its two keys; in value it may go past them.)
                let handle = |n: usize, rest: f64| e[n].get(dim).or(e[n].first()).copied().unwrap_or(rest);
                let curve = [handle(0, 0.0).clamp(0.0, 1.0), handle(1, 0.0), handle(2, 1.0).clamp(0.0, 1.0), handle(3, 1.0)];
                if curve != [0.0, 0.0, 1.0, 1.0] {
                    key.insert("interp".into(), json!("cubic"));
                    for (name, v) in ["easeX1", "easeY1", "easeX2", "easeY2"].into_iter().zip(curve) {
                        key.insert(name.into(), num(v));
                    }
                }
            }
            J::Object(key)
        };
        let keys: Vec<J> = keys.iter().enumerate().map(key).collect();
        self.tracks.push(json!({ "type": "Track", "target": target, "property": property, "children": keys }));
    }

    /// Sets a number of `o` from one dimension of a Lottie property, `scale` times what
    /// the file says (left out where it is `rest`, the value Zuri gives it unasked), with
    /// a Track where the property is keyed.
    #[allow(clippy::too_many_arguments)]
    fn set(&mut self, o: &mut Obj, hint: &str, property: &str, prop: &J, dim: usize, rest: f64, time: f64, scale: f64) {
        if prop.is_null() {
            return;
        }
        let keys = self.keys(prop);
        let value = |k: &Kf| num(scale * k.s.get(dim).copied().unwrap_or(0.0));
        if keys.len() > 1 {
            let target = self.name(o, hint);
            self.track(&target, property, &keys, dim, time, &|_, k| ("value", value(k)));
            o.insert(property.into(), value(&keys[0]));
        } else if keys[0].s.len() > dim && value(&keys[0]) != json!(rest) {
            o.insert(property.into(), value(&keys[0]));
        }
    }

    /// A place, from a Lottie position: one property of two numbers, or one for each.
    fn place(&mut self, o: &mut Obj, hint: &str, p: &J, time: f64, scale: f64) {
        if p["s"] == true || p["s"] == 1 {
            self.set(o, hint, "x", &p["x"], 0, 0.0, time, scale);
            self.set(o, hint, "y", &p["y"], 0, 0.0, time, scale);
        } else {
            self.set(o, hint, "x", p, 0, 0.0, time, scale);
            self.set(o, hint, "y", p, 1, 0.0, time, scale);
            if frames(p).iter().any(|k| nums(&k["to"]).iter().chain(&nums(&k["ti"])).any(|v| *v != 0.0)) {
                self.notes.insert("curved motion paths (the way from key to key is straight)");
            }
        }
    }

    /// `inside` in a Node that is moved, turned, scaled and (with `fades`) faded as a
    /// Lottie transform says, the anchor taken off by a Node of its own. A transform that
    /// does nothing leaves `inside` as it is.
    fn transform(&mut self, ks: &J, hint: &str, fades: bool, time: f64, mut inside: Vec<J>) -> Vec<J> {
        let (mut node, mut anchor) = (obj("Node"), obj("Node"));
        self.place(&mut node, hint, &ks["p"], time, 1.0);
        self.set(&mut node, hint, "rotation", if ks["r"].is_null() { &ks["rz"] } else { &ks["r"] }, 0, 0.0, time, 1.0);
        self.set(&mut node, hint, "scaleX", &ks["s"], 0, 1.0, time, 0.01);
        self.set(&mut node, hint, "scaleY", &ks["s"], 1, 1.0, time, 0.01);
        if fades {
            self.set(&mut node, hint, "opacity", &ks["o"], 0, 1.0, time, 0.01);
        }
        if self.keys(&ks["sk"]).iter().any(|k| k.s.first().is_some_and(|v| *v != 0.0)) {
            self.notes.insert("skew");
        }
        self.place(&mut anchor, hint, &ks["a"], time, -1.0);
        for mut wrap in [anchor, node] {
            if wrap.len() > 1 {
                wrap.insert("children".into(), J::Array(inside));
                inside = vec![J::Object(wrap)];
            }
        }
        inside
    }

    /// A Rectangle, Ellipse, Star, Polygon or Path for a Lottie shape.
    fn geometry(&mut self, item: &J, time: f64) -> J {
        let (ty, hint) = (item["ty"].as_str().unwrap_or(""), item["nm"].as_str().unwrap_or("path"));
        let mut o = obj(match ty {
            "rc" => "Rectangle",
            "el" => "Ellipse",
            "sr" if item["sy"] == 2 => "Polygon",
            "sr" => "Star",
            _ => "Path",
        });
        // (A path has no place of its own: none is set.)
        self.place(&mut o, hint, &item["p"], time, 1.0);
        match ty {
            "rc" | "el" => {
                self.set(&mut o, hint, "width", &item["s"], 0, 0.0, time, 1.0);
                self.set(&mut o, hint, "height", &item["s"], 1, 0.0, time, 1.0);
                self.set(&mut o, hint, "radius", &item["r"], 0, 0.0, time, 1.0);
            }
            "sr" => {
                self.set(&mut o, hint, "width", &item["or"], 0, 0.0, time, 2.0);
                self.set(&mut o, hint, "height", &item["or"], 0, 0.0, time, 2.0);
                self.set(&mut o, hint, "rotation", &item["r"], 0, 0.0, time, 1.0);
                let first = |l: &mut Self, prop: &J| l.keys(prop)[0].s.first().copied().unwrap_or(0.0);
                let (points, outer, inner) = (first(self, &item["pt"]), first(self, &item["or"]), first(self, &item["ir"]));
                o.insert("points".into(), json!(points.round().clamp(3.0, 1e3) as u32));
                let star = o["type"] == "Star";
                if star && outer > 0.0 {
                    o.insert("innerRadius".into(), num((inner / outer).clamp(0.0, 1.0)));
                }
                // (Zuri's inner radius is a part of the outer one: keyed, either moves it.)
                let keyed = [(&item["pt"], true), (&item["ir"], star), (&item["or"], star)].iter().any(|(prop, counts)| *counts && !frames(prop).is_empty());
                let round = [&item["os"], &item["is"]].iter().any(|prop| keys(prop).iter().any(|k| k.s.first().is_some_and(|v| *v != 0.0)));
                if keyed || round {
                    self.notes
                        .insert("a star's roundness, and keys on how many points it has or on its radii (the inner one stays the same part of the outer)");
                }
            }
            // A path that is keyed morphs: Zuri keys the path data itself.
            _ => {
                let (shapes, closed) = (frames(&item["ks"]), item["closed"] == true);
                let d = |n: usize| path(&value(shapes, n)[0], closed);
                o.insert("d".into(), json!(if shapes.is_empty() { path(&item["ks"]["k"], closed) } else { d(0) }));
                if shapes.len() > 1 {
                    let target = self.name(&mut o, hint);
                    self.track(&target, "d", &keys(&item["ks"]), 0, time, &|n, _| ("textValue", json!(d(n))));
                }
            }
        }
        J::Object(o)
    }

    /// The color of a Fill or Stroke: the Lottie color, its opacity as the alpha.
    fn color(&mut self, o: &mut Obj, hint: &str, style: &J, time: f64) {
        let (colors, fades) = (self.keys(&style["c"]), self.keys(&style["o"]));
        let alpha = |k: &Kf| k.s.first().copied().unwrap_or(100.0) / 100.0;
        o.insert("color".into(), hex(&colors[0].s, alpha(&fades[0])));
        if colors.len() > 1 {
            if fades.len() > 1 {
                self.notes.insert("a paint's opacity keyed along with its color (the first opacity is kept)");
            }
            let target = self.name(o, hint);
            self.track(&target, "color", &colors, 0, time, &|_, k| ("colorValue", hex(&k.s, alpha(&fades[0]))));
        } else if fades.len() > 1 {
            let target = self.name(o, hint);
            self.track(&target, "color", &fades, 0, time, &|_, k| ("colorValue", hex(&colors[0].s, alpha(k))));
        }
    }

    /// The gradient of a Fill or Stroke: a stop wherever Lottie has one for a color or
    /// for an opacity, each with both.
    fn gradient(&mut self, o: &mut Obj, hint: &str, style: &J, time: f64) {
        let (stops, fades) = (self.keys(&style["g"]["k"]), self.keys(&style["o"]));
        if stops.len() > 1 || fades.len() > 1 {
            self.notes.insert("keys on a gradient's colors or opacity (the first are kept)");
        }
        let flat = &stops[0].s;
        let count = (style["g"]["p"].as_u64().unwrap_or(0) as usize).min(flat.len() / 4);
        let colors: Vec<(f64, Vec<f64>)> = flat[..count * 4].as_chunks::<4>().0.iter().map(|c| (c[0], c[1..].to_vec())).collect();
        let alphas: Vec<(f64, Vec<f64>)> = flat[count * 4..].as_chunks::<2>().0.iter().map(|a| (a[0], vec![a[1]])).collect();
        // What a list of stops is at a place: the nearest one's beyond the ends, between two in proportion.
        let at = |stops: &[(f64, Vec<f64>)], t: f64, rest: f64| -> Vec<f64> {
            let after = stops.iter().position(|s| s.0 >= t).unwrap_or(stops.len());
            match (after.checked_sub(1).map(|n| &stops[n]), stops.get(after)) {
                (Some(a), Some(b)) if b.0 > a.0 => a.1.iter().zip(&b.1).map(|(x, y)| x + (y - x) * (t - a.0) / (b.0 - a.0)).collect(),
                (_, Some(s)) | (Some(s), None) => s.1.clone(),
                (None, None) => vec![rest],
            }
        };
        let opacity = fades[0].s.first().copied().unwrap_or(100.0) / 100.0;
        // Every color stop with its own color (two at one place are a hard edge), and a
        // stop more where only the opacity has one.
        let mut all: Vec<(f64, J)> = colors.iter().map(|(t, c)| (*t, hex(c, at(&alphas, *t, 1.0)[0] * opacity))).collect();
        all.extend(alphas.iter().filter(|a| colors.iter().all(|c| c.0 != a.0)).map(|a| (a.0, hex(&at(&colors, a.0, 0.0), a.1[0] * opacity))));
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        let stops: Vec<J> = all.into_iter().map(|(t, color)| json!({ "type": "GradientStop", "position": num(t), "color": color })).collect();
        let radial = style["t"] == 2;
        let mut g = obj(if radial { "RadialGradient" } else { "LinearGradient" });
        self.set(&mut g, hint, "x0", &style["s"], 0, 0.0, time, 1.0);
        self.set(&mut g, hint, "y0", &style["s"], 1, 0.0, time, 1.0);
        if radial {
            let (from, to) = (self.keys(&style["s"]), self.keys(&style["e"]));
            let d = |n: usize| to[0].s.get(n).copied().unwrap_or(0.0) - from[0].s.get(n).copied().unwrap_or(0.0);
            g.insert("radius".into(), num(d(0).hypot(d(1))));
            if from.len() > 1 || to.len() > 1 {
                self.notes.insert("keys on a round gradient's size (the first is kept)");
            }
        } else {
            self.set(&mut g, hint, "x1", &style["e"], 0, 0.0, time, 1.0);
            self.set(&mut g, hint, "y1", &style["e"], 1, 0.0, time, 1.0);
        }
        g.insert("children".into(), J::Array(stops));
        o.insert("children".into(), json!([g]));
    }

    /// A Fill or a Stroke for a Lottie style. `trim`: the trim that cuts what it paints.
    fn paint(&mut self, style: &J, trim: Option<&J>, time: f64) -> J {
        let ty = style["ty"].as_str().unwrap_or("");
        let stroke = matches!(ty, "st" | "gs");
        let hint = style["nm"].as_str().unwrap_or(if stroke { "stroke" } else { "fill" });
        let mut o = obj(if stroke { "Stroke" } else { "Fill" });
        if matches!(ty, "gf" | "gs") {
            self.gradient(&mut o, hint, style, time);
        } else {
            self.color(&mut o, hint, style, time);
        }
        if !stroke {
            if style["r"] == 2 {
                o.insert("fillRule".into(), json!("evenodd"));
            }
            if trim.is_some() {
                self.notes.insert("a trim on a fill (strokes are trimmed)");
            }
            return J::Object(o);
        }
        self.set(&mut o, hint, "thickness", &style["w"], 0, 1.0, time, 1.0);
        for (key, value, names) in [("cap", &style["lc"], ["round", "square"]), ("join", &style["lj"], ["round", "bevel"])] {
            if let Some(name) = value.as_u64().and_then(|v| names.get(v.wrapping_sub(2) as usize)) {
                o.insert(key.into(), json!(name));
            }
        }
        let dashes = style["d"].as_array().map_or(&[][..], Vec::as_slice);
        for (n, key) in [("d", "dash"), ("g", "gap"), ("o", "dashOffset")] {
            let mut of = dashes.iter().filter(|d| d["n"] == n);
            if let Some(d) = of.next() {
                self.set(&mut o, hint, key, &d["v"], 0, 0.0, time, 1.0);
            }
            if of.next().is_some() {
                self.notes.insert("dash patterns of more than one dash and gap (the first are kept)");
            }
        }
        if let Some(trim) = trim {
            self.set(&mut o, hint, "trimStart", &trim["s"], 0, 0.0, time, 0.01);
            self.set(&mut o, hint, "trimEnd", &trim["e"], 0, 1.0, time, 0.01);
            self.set(&mut o, hint, "trimOffset", &trim["o"], 0, 0.0, time, 1.0 / 360.0);
            // (Lottie trims each path by itself unless told to go through them one after another.)
            if trim["m"] != 2 {
                o.insert("trimMode".into(), json!("synced"));
            }
        }
        J::Object(o)
    }

    /// A Shape of some Lottie shapes and the styles that paint them, the first on top
    /// (Zuri paints the last one last). A trim is a Stroke's in Zuri, for all that the
    /// Stroke paints: where it cuts only some of that, all is cut.
    fn shape(&mut self, shapes: &[Cut], styles: &[&J], time: f64) -> J {
        let trim = shapes.iter().find_map(|s| s.1);
        if shapes.iter().any(|s| s.1.is_none()) && trim.is_some() {
            self.notes.insert("a trim on some of the shapes a stroke paints (all of them are trimmed)");
        }
        let mut children: Vec<J> = shapes.iter().map(|s| self.geometry(s.0, time)).collect();
        children.extend(styles.iter().rev().map(|style| self.paint(style, trim, time)));
        json!({ "type": "Shape", "children": children })
    }

    /// What a list of Lottie shapes draws, in Zuri's order (the last on top). A style
    /// paints the shapes before it in its list, those of the groups among them too:
    /// `outer` is the styles of the lists around that come after this one. A trim cuts
    /// the shapes before it likewise: `cut` is the one that cuts this list from outside.
    // ponytail: a style of a list around paints a group's shapes from inside the group,
    // under all that the group draws itself: right where the groups do not overlap and
    // their shapes are not one figure with holes. Folding a group's shapes into the
    // Shape of the list around (its transform into their own) would be exact. And the
    // shapes that several styles paint are written (with their Tracks) once for each
    // run of styles: one Shape cannot paint a part of its geometry.
    fn scope(&mut self, items: &[J], outer: &[&J], cut: Option<&J>, time: f64) -> Vec<J> {
        let items: Vec<&J> = items.iter().filter(|item| item["hd"] != true).collect();
        let style = |item: &J| matches!(item["ty"].as_str(), Some("fl" | "st" | "gf" | "gs"));
        let trim = |n: usize| items[n + 1..].iter().copied().find(|item| item["ty"] == "tm").or(cut);
        // As Lottie has them, the first on top.
        let mut out: Vec<J> = Vec::new();
        let mut shapes: Vec<Cut> = Vec::new();
        let mut n = 0;
        while n < items.len() {
            let item = items[n];
            match item["ty"].as_str() {
                Some("rc" | "el" | "sr" | "sh") => shapes.push((item, trim(n))),
                Some("gr") => {
                    let later: Vec<&J> = items[n + 1..].iter().copied().filter(|after| style(after)).chain(outer.iter().copied()).collect();
                    let it = item["it"].as_array().map_or(&[][..], Vec::as_slice);
                    let inside = self.scope(it, &later, trim(n), time);
                    let tr = it.iter().find(|i| i["ty"] == "tr").unwrap_or(&J::Null);
                    out.extend(self.transform(tr, item["nm"].as_str().unwrap_or("group"), true, time, inside).into_iter().rev());
                }
                // Styles one after another paint the same shapes: one Shape.
                Some(_) if style(item) => {
                    let run = items[n..].iter().take_while(|next| style(next)).count();
                    if !shapes.is_empty() {
                        out.push(self.shape(&shapes, &items[n..n + run], time));
                    }
                    n += run - 1;
                }
                Some("tm" | "tr") | None => {}
                Some(_) => drop(self.notes.insert("shape modifiers other than trims (rounded corners, repeaters, merges and the like)")),
            }
            n += 1;
        }
        if !shapes.is_empty() && !outer.is_empty() {
            out.push(self.shape(&shapes, outer, time));
        }
        out.reverse();
        out
    }

    /// The layers of a composition, in Zuri's order (the last on top). `time`: where
    /// the composition's frame 0 is on the artboard's timeline.
    // ponytail: a parent's Nodes and Tracks are written again for each of its children
    // (a rig of one keyed null and thirty layers has its keys thirty times), because the
    // children keep their own places in the order of the layers. Children that are next
    // to each other in that order could share one.
    fn layers(&mut self, layers: &[J], time: f64, depth: u32) -> Vec<J> {
        let mut out = Vec::new();
        for layer in layers.iter().rev() {
            let hint = layer["nm"].as_str().unwrap_or("layer");
            // When it shows, on the artboard's timeline: one that never does is left out.
            let (from, to) = (layer["ip"].as_f64().map_or(0.0, |t| t + time), layer["op"].as_f64().map_or(self.end, |t| t + time));
            if layer["hd"] == true || to <= 0.0 || from >= self.end {
                continue;
            }
            if self.room == 0 {
                self.notes.insert("layers beyond the first 20000 (precompositions in precompositions)");
                break;
            }
            self.room -= 1;
            if layer["td"] == 1 || !layer["tt"].is_null() {
                self.notes.insert("track mattes (the matte is left out, and what it cut is drawn whole)");
                if layer["td"] == 1 {
                    continue;
                }
            }
            let content = match layer["ty"].as_i64() {
                Some(4) => self.scope(layer["shapes"].as_array().map_or(&[], Vec::as_slice), &[], None, time),
                Some(1) => {
                    let (w, h) = (layer["sw"].as_f64().unwrap_or(0.0), layer["sh"].as_f64().unwrap_or(0.0));
                    let color = layer["sc"].as_str().and_then(zuri_core::json::parse_color).unwrap_or_else(|| {
                        self.notes.insert("a solid's color that is not written #rrggbb (it is black)");
                        0xff00_0000
                    });
                    vec![json!({ "type": "Shape", "x": w / 2.0, "y": h / 2.0, "children": [
                        { "type": "Rectangle", "width": w, "height": h }, { "type": "Fill", "color": zuri_core::json::format_color(color) } ] })]
                }
                Some(0) => {
                    let asset = self.assets.iter().find(|a| a["id"] == layer["refId"] && a["id"].is_string());
                    let Some(inner) = asset.and_then(|a| a["layers"].as_array()).filter(|_| depth < 16) else {
                        self.notes.insert("a precomposition that the file does not have, or one more than 16 deep");
                        continue;
                    };
                    if layer["sr"].as_f64().is_some_and(|sr| sr != 1.0) || !layer["tm"].is_null() {
                        self.notes.insert("time stretch and time remapping of a precomposition (it plays at its own pace)");
                    }
                    self.layers(inner, time + layer["st"].as_f64().unwrap_or(0.0), depth + 1)
                }
                // (A null layer draws nothing: it is there to be a parent.)
                ty => {
                    self.notes.extend(match ty {
                        Some(3) => None,
                        Some(2) => Some("image layers"),
                        Some(5) => Some("text layers"),
                        _ => Some("layers that are neither shapes, solids, nulls nor precompositions"),
                    });
                    continue;
                }
            };
            for (key, note) in [("masksProperties", "masks"), ("ef", "effects")] {
                if layer[key].as_array().is_some_and(|a| !a.is_empty()) {
                    self.notes.insert(note);
                }
            }
            for (key, note) in
                [("ao", "auto-orient along a motion path"), ("bm", "blend modes of layers"), ("ddd", "3D layers (turned about the z axis alone)")]
            {
                if layer[key].as_f64().is_some_and(|v| v != 0.0) {
                    self.notes.insert(note);
                }
            }
            let mut nodes = self.transform(&layer["ks"], hint, true, time, content);
            // A parent hands on its transform and not its opacity. (Not more of them than there are layers: a ring of parents ends.)
            let mut parent = layer["parent"].as_i64();
            for _ in 0..layers.len() {
                let Some(above) = layers.iter().find(|l| parent.is_some() && l["ind"].as_i64() == parent) else { break };
                nodes = self.transform(&above["ks"], above["nm"].as_str().unwrap_or("layer"), false, time, nodes);
                parent = above["parent"].as_i64();
            }
            // One that comes in late or goes early is there for that long only.
            if from > 0.0 || to < self.end {
                let mut span = obj("Node");
                let target = self.name(&mut span, hint);
                let key = |frame: f64, value: u8| json!({ "type": "Key", "frame": num(frame), "value": value, "interp": "hold" });
                let mut keys = vec![key(0.0, (from <= 0.0) as u8)];
                keys.extend((from > 0.0).then(|| key(from, 1)));
                keys.extend((to < self.end).then(|| key(to, 0)));
                self.tracks.push(json!({ "type": "Track", "target": target, "property": "opacity", "children": keys }));
                span.insert("opacity".into(), json!((from <= 0.0) as u8));
                span.insert("children".into(), J::Array(nodes));
                nodes = vec![J::Object(span)];
            }
            out.extend(nodes);
        }
        out
    }
}

/// A Lottie animation as a Zuri document, and what of it was not imported.
pub fn lottie(data: &[u8], name: &str) -> Result<(J, Vec<String>)> {
    let root: J = serde_json::from_slice(data).map_err(|e| Error(format!("this is not JSON: {e}")))?;
    let (Some(layers), Some(w), Some(h)) = (root["layers"].as_array(), root["w"].as_f64(), root["h"].as_f64()) else {
        return Err(Error("this JSON is not a Lottie animation: it has no `layers`, `w` and `h`".into()));
    };
    let start = root["ip"].as_f64().unwrap_or(0.0);
    let end = (root["op"].as_f64().unwrap_or(start) - start).max(1.0);
    let assets = root["assets"].as_array().map_or(&[][..], Vec::as_slice);
    let mut l = Lottie { assets, end, room: 20_000, tracks: Vec::new(), notes: BTreeSet::new(), named: 0 };
    let mut children = l.layers(layers, -start, 0);
    if !l.tracks.is_empty() {
        let fps = root["fr"].as_f64().filter(|fps| *fps > 0.0).unwrap_or(60.0);
        children.push(json!({ "type": "Animation", "name": name, "fps": fps, "duration": num(end), "loop": "loop", "children": l.tracks }));
    }
    let doc = json!({ "zuri": 1, "children": [{ "type": "Artboard", "name": name, "width": w, "height": h, "children": children }] });
    Ok((doc, l.notes.into_iter().map(String::from).collect()))
}
