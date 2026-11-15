//! A playable instance of one artboard.
//!
//! Per frame the host calls `advance(dt)` (state machine -> animations -> data binds ->
//! transforms) and, if that returned `true`, `render(..)`. `false` means nothing changed
//! and the previous frame is still valid, so idle scenes cost nothing.

use crate::file::{Anim, Artboard, File, Image, Layer, PVal, State, Track, NONE};
use crate::geom::{self, lerp_path, parse_svg_path, pt, Contour, Mat, Path, P};
use crate::schema::{self, p, t, Kind};
use crate::{Raster, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// How an artboard is scaled into a surface (always centered).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    Contain,
    Cover,
    Fill,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Move,
    Down,
    Up,
    /// The pointer left the surface.
    Exit,
}

#[derive(Clone)]
struct VmVal {
    num: f32,
    color: u32,
    text: Arc<str>,
    fired: bool,
}

#[derive(Clone, Default)]
struct LayerRt {
    cur: usize,
    time: f64,
    from: Option<(usize, f64)>,
    mix: f32,
    mix_dur: f32,
    /// What the cross-fade's target properties held when the transition fired.
    base: Vec<(u32, u16, PVal)>,
}

enum Player {
    None,
    // Clocks are f64: f32 seconds lose frame resolution after a few hours of looping.
    Anim { idx: usize, time: f64 },
    Machine { idx: usize, layers: Vec<LayerRt> },
}

/// What a host must render for a `ShaderPaint` (see `Instance::shader_requests`).
/// This layout is the C ABI's `ZuriShaderRequest` (zuri-ffi/include/zuri.h): keep the
/// field order and types identical to the header.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaderRequest {
    /// File index of the ShaderPaint; pass it back to `set_shader_texture`.
    pub paint: u32,
    /// File index of the ShaderAsset.
    pub asset: u32,
    pub width: u32,
    pub height: u32,
    pub params: [f32; 4],
    pub time: f32,
}

/// Most nested artboard instances one instance may create, over all levels.
const MAX_NESTED: u32 = 4096;

pub struct Instance {
    pub(crate) file: Arc<File>,
    pub(crate) ab: usize,
    props: Vec<Vec<(u16, PVal)>>,
    /// Node -> artboard space.
    pub(crate) world: Vec<Mat>,
    /// Cached local outlines of geometry and text nodes.
    pub(crate) geo: Vec<Option<Arc<Path>>>,
    vm: Vec<VmVal>,
    player: Player,
    pub(crate) nested: Vec<(u32, Instance)>,
    events: Vec<u32>,
    /// Artboard -> surface pixels, from the last render. Pointer input is mapped through it.
    pub(crate) view: Mat,
    dirty: bool,
    hover: Vec<bool>,
    pressed: Vec<bool>,
    pub(crate) time: f64,
    /// `File::image_gen` as last seen: a late `set_image` must redraw a settled scene.
    image_gen: u32,
    pub(crate) textures: HashMap<u32, Arc<Image>>,
    pub(crate) requests: Vec<ShaderRequest>,
    has_shader: bool,
}

impl Instance {
    pub fn new(file: &Arc<File>, artboard: usize) -> Result<Instance> {
        Instance::with_depth(file, artboard, 0, &mut { MAX_NESTED })
    }

    fn with_depth(file: &Arc<File>, artboard: usize, depth: u32, budget: &mut u32) -> Result<Instance> {
        let Some(ab) = file.artboards.get(artboard) else { bail!("artboard index {artboard} out of range ({} artboards)", file.artboards.len()) };
        let mut props = Vec::with_capacity(ab.nodes.len());
        for n in &ab.nodes {
            let mut list = Vec::new();
            for (key, raw) in &file.objects[n.obj as usize].props {
                let Some(def) = schema::prop(*key) else { continue };
                use crate::format::Value;
                let v = match (def.kind, raw) {
                    (Kind::Float, Value::Float(v)) if v.is_finite() => PVal::F(*v),
                    (Kind::Color, Value::Color(c)) => PVal::C(*c),
                    (Kind::Str, Value::Bytes(b)) => match std::str::from_utf8(b) {
                        Ok(s) if *key == p::D => PVal::P(Arc::new(parse_svg_path(s).unwrap_or_default())),
                        Ok(s) => PVal::S(s.into()),
                        Err(_) => continue,
                    },
                    (Kind::Uint | Kind::Bool | Kind::Ref | Kind::PropKey | Kind::Enum(_), Value::Uint(v)) => PVal::U(*v as u32),
                    _ => continue,
                };
                list.push((*key, v));
            }
            props.push(list);
        }
        let mut inst = Instance {
            file: file.clone(),
            ab: artboard,
            props,
            world: vec![Mat::IDENTITY; ab.nodes.len()],
            geo: vec![None; ab.nodes.len()],
            vm: ab.vm.iter().map(|v| VmVal { num: v.num, color: v.color, text: v.text.clone(), fired: false }).collect(),
            player: Player::None,
            nested: Vec::new(),
            events: Vec::new(),
            view: Mat::IDENTITY,
            dirty: true,
            hover: Vec::new(),
            pressed: Vec::new(),
            time: 0.0,
            image_gen: 0,
            textures: HashMap::new(),
            requests: Vec::new(),
            has_shader: ab.nodes.iter().any(|n| n.ty == t::SHADER_PAINT),
        };
        // ponytail: nesting is capped at 8 levels, which also stops an artboard that nests itself;
        // `budget` caps the total, because 8 levels of 8 children would be 16 million instances.
        if depth < 8 {
            for (i, n) in ab.nodes.iter().enumerate() {
                if n.ty != t::NESTED_ARTBOARD {
                    continue;
                }
                let target = inst.u(i as u32, p::ARTBOARD_REF);
                if let Some(idx) = file.artboard_index(target) {
                    if *budget == 0 {
                        bail!("more than {MAX_NESTED} nested artboard instances (does an artboard nest itself?)");
                    }
                    *budget -= 1;
                    let mut child = Instance::with_depth(file, idx, depth + 1, budget)?;
                    child.autoplay();
                    inst.has_shader |= child.has_shader;
                    inst.nested.push((i as u32, child));
                }
            }
        }
        inst.update_world();
        Ok(inst)
    }

    pub(crate) fn def(&self) -> &Artboard {
        &self.file.artboards[self.ab]
    }
    pub fn file(&self) -> &Arc<File> {
        &self.file
    }
    pub fn width(&self) -> f32 {
        self.f(0, p::WIDTH)
    }
    pub fn height(&self) -> f32 {
        self.f(0, p::HEIGHT)
    }
    pub fn animation_names(&self) -> Vec<&str> {
        self.def().anims.iter().map(|a| a.name.as_str()).collect()
    }
    pub fn state_machine_names(&self) -> Vec<&str> {
        self.def().machines.iter().map(|m| m.name.as_str()).collect()
    }

    // ---- property access -------------------------------------------------------------

    fn get(&self, n: u32, key: u16) -> Option<&PVal> {
        self.props[n as usize].iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    pub(crate) fn f(&self, n: u32, key: u16) -> f32 {
        match self.get(n, key) {
            Some(PVal::F(v)) => *v,
            _ => schema::default(key),
        }
    }
    pub(crate) fn u(&self, n: u32, key: u16) -> u32 {
        match self.get(n, key) {
            Some(PVal::U(v)) => *v,
            // A reference that was never set points at nothing, not at object 0.
            _ if matches!(key, p::ASSET | p::ARTBOARD_REF) => NONE,
            _ => schema::default(key) as u32,
        }
    }
    pub(crate) fn c(&self, n: u32, key: u16) -> u32 {
        match self.get(n, key) {
            Some(PVal::C(v)) => *v,
            _ => schema::DEFAULT_COLOR,
        }
    }
    #[cfg_attr(not(feature = "text"), allow(dead_code))]
    pub(crate) fn s(&self, n: u32, key: u16) -> &str {
        match self.get(n, key) {
            Some(PVal::S(v)) => v,
            _ => "",
        }
    }
    /// Node index of the scene object a `Ref` property points at.
    pub(crate) fn node_ref(&self, n: u32, key: u16) -> Option<u32> {
        match self.get(n, key) {
            Some(PVal::U(obj)) => self.def().local.get(obj).copied(),
            _ => None,
        }
    }

    /// Write a property. Marks the instance dirty only when the value really changes.
    fn set(&mut self, n: u32, key: u16, v: PVal) {
        let list = &mut self.props[n as usize];
        match list.iter_mut().find(|(k, _)| *k == key) {
            Some((_, old)) if *old == v => return,
            Some((_, old)) => *old = v,
            None => list.push((key, v)),
        }
        // Transform and opacity do not change a node's local outline; anything else might.
        if !matches!(key, p::X | p::Y | p::ROTATION | p::SCALE_X | p::SCALE_Y | p::OPACITY) {
            self.geo[n as usize] = None;
        }
        self.dirty = true;
    }

    // ---- playback --------------------------------------------------------------------

    /// Play one animation directly (no state machine).
    pub fn play_animation(&mut self, idx: usize) -> bool {
        let ok = idx < self.def().anims.len();
        if ok {
            self.player = Player::Anim { idx, time: 0.0 };
            self.dirty = true;
        }
        ok
    }

    pub fn play_state_machine(&mut self, idx: usize) -> bool {
        let Some(m) = self.def().machines.get(idx) else { return false };
        let layers = m.layers.iter().map(|l| LayerRt { cur: l.entry, ..Default::default() }).collect();
        let n = m.listeners.len();
        self.player = Player::Machine { idx, layers };
        self.hover = vec![false; n];
        self.pressed = vec![false; n];
        self.dirty = true;
        true
    }

    /// Play the first state machine, or failing that the first animation.
    pub fn autoplay(&mut self) {
        if !self.play_state_machine(0) {
            self.play_animation(0);
        }
    }

    /// Advance by `dt` seconds. Returns `true` if the picture may have changed.
    pub fn advance(&mut self, dt: f32) -> bool {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.time += dt as f64;
        let image_gen = self.file.image_gen.load(std::sync::atomic::Ordering::Relaxed);
        self.dirty |= std::mem::replace(&mut self.image_gen, image_gen) != image_gen;
        let mut active = match std::mem::replace(&mut self.player, Player::None) {
            Player::None => false,
            Player::Anim { idx, time } => {
                let file = self.file.clone();
                let a = &file.artboards[self.ab].anims[idx];
                let time = time + (dt * a.speed) as f64;
                let (frame, finished) = map_frame(a, time * a.fps as f64);
                self.apply(a, frame, 1.0);
                self.player = Player::Anim { idx, time };
                !finished
            }
            Player::Machine { idx, mut layers } => {
                let active = self.advance_machine(idx, &mut layers, dt);
                self.player = Player::Machine { idx, layers };
                active
            }
        };
        for v in &mut self.vm {
            v.fired = false;
        }
        self.apply_binds();
        for (_, child) in &mut self.nested {
            active |= child.advance(dt);
        }
        self.update_world();
        // Shaders run on time, so a scene with one keeps redrawing, but only for hosts that
        // actually draw shaders (they have supplied a texture). Others settle normally.
        std::mem::take(&mut self.dirty) || active || (self.has_shader && !self.textures.is_empty())
    }

    fn apply(&mut self, a: &Anim, frame: f32, mix: f32) {
        for tr in &a.tracks {
            let Some(mut v) = sample(tr, frame) else { continue };
            if mix < 1.0 {
                v = match (self.get(tr.node, tr.prop), v) {
                    (cur, PVal::F(b)) => {
                        let a = if let Some(PVal::F(a)) = cur { *a } else { schema::default(tr.prop) };
                        PVal::F(a + (b - a) * mix)
                    }
                    (cur, PVal::C(b)) => PVal::C(lerp_color(if let Some(PVal::C(a)) = cur { *a } else { schema::DEFAULT_COLOR }, b, mix)),
                    (Some(PVal::P(a)), PVal::P(b)) => PVal::P(Arc::new(lerp_path(a, &b, mix))),
                    (Some(cur), _) if mix < 0.5 => cur.clone(),
                    (_, v) => v,
                };
            }
            self.set(tr.node, tr.prop, v);
        }
    }

    fn advance_machine(&mut self, idx: usize, layers: &mut [LayerRt], dt: f32) -> bool {
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        let mut active = false;
        for (layer, rt) in ab.machines[idx].layers.iter().zip(layers) {
            if layer.states.is_empty() {
                continue;
            }
            // Transitions are decided on the state as the last frame left it, then time moves,
            // so a transition that fires now already blends by `dt` in this same frame.
            // A trigger drives at most one transition per layer per frame.
            let mut spent: Vec<usize> = Vec::new();
            // Bounded so a cycle of unconditional transitions cannot hang the frame.
            for _ in 0..8 {
                let candidates = layer.any.iter().flat_map(|s| &layer.states[*s].transitions).filter(|tr| tr.to != rt.cur);
                let fired = candidates.chain(&layer.states[rt.cur].transitions).find(|tr| {
                    let waited = tr.exit_time <= 0.0
                        || layer.states[rt.cur].anim.is_none_or(|a| {
                            let a = &ab.anims[a];
                            a.duration <= 0.0 || rt.time * (a.fps * a.speed / a.duration) as f64 >= tr.exit_time as f64
                        });
                    waited
                        && tr.conds.iter().all(|c| {
                            let v = &self.vm[c.slot];
                            if ab.vm[c.slot].ty == 2 {
                                return v.fired && !spent.contains(&c.slot);
                            }
                            match c.op {
                                0 => v.num == c.value,
                                1 => v.num != c.value,
                                2 => v.num < c.value,
                                3 => v.num <= c.value,
                                4 => v.num > c.value,
                                _ => v.num >= c.value,
                            }
                        })
                });
                let Some(tr) = fired else { break };
                spent.extend(tr.conds.iter().map(|c| c.slot).filter(|s| ab.vm[*s].ty == 2));
                self.events.extend(tr.emit);
                rt.from = (tr.blend > 0.0).then_some((rt.cur, rt.time));
                rt.base = if tr.blend > 0.0 { self.pose(ab, &layer.states[tr.to]) } else { Vec::new() };
                (rt.cur, rt.time, rt.mix, rt.mix_dur) = (tr.to, 0.0, 0.0, tr.blend);
                self.dirty = true;
            }
            rt.time += (dt * layer.states[rt.cur].speed) as f64;
            if let Some((from, time)) = &mut rt.from {
                *time += (dt * layer.states[*from].speed) as f64;
                rt.mix += dt / rt.mix_dur;
                if rt.mix >= 1.0 {
                    rt.from = None;
                    rt.base.clear();
                }
            }
            match rt.from {
                Some((from, time)) => {
                    // Mix from the pose the transition started at, not from last frame's mix:
                    // otherwise a property the outgoing state does not animate compounds.
                    for (n, key, v) in &rt.base {
                        self.set(*n, *key, v.clone());
                    }
                    self.apply_state(ab, layer, from, time, 1.0);
                    self.apply_state(ab, layer, rt.cur, rt.time, rt.mix);
                    active = true;
                }
                None => active |= self.apply_state(ab, layer, rt.cur, rt.time, 1.0),
            }
        }
        active
    }

    /// The current value of every property `st`'s animations drive.
    fn pose(&self, ab: &Artboard, st: &State) -> Vec<(u32, u16, PVal)> {
        let tracks = st.anim.into_iter().chain(st.blend.iter().map(|b| b.0)).flat_map(|a| &ab.anims[a].tracks);
        let value = |tr: &Track| match (self.get(tr.node, tr.prop), schema::prop(tr.prop)?.kind) {
            (Some(v), _) => Some(v.clone()),
            (None, Kind::Float) => Some(PVal::F(schema::default(tr.prop))),
            (None, Kind::Color) => Some(PVal::C(schema::DEFAULT_COLOR)),
            _ => None,
        };
        tracks.filter_map(|tr| Some((tr.node, tr.prop, value(tr)?))).collect()
    }

    /// Apply a state's animation(s) at `time` seconds. Returns whether it is still moving.
    fn apply_state(&mut self, ab: &Artboard, layer: &Layer, state: usize, time: f64, mix: f32) -> bool {
        let st = &layer.states[state];
        let play = |this: &mut Instance, anim: usize, mix: f32| {
            let a = &ab.anims[anim];
            let (frame, finished) = map_frame(a, time * (a.fps * a.speed) as f64);
            this.apply(a, frame, mix);
            !finished
        };
        match st.kind {
            0 => st.anim.is_some_and(|a| play(self, a, mix)),
            4 if !st.blend.is_empty() => {
                let v = st.input.map_or(0.0, |s| self.vm[s].num);
                let hi = st.blend.iter().position(|b| b.1 >= v).unwrap_or(st.blend.len() - 1);
                let lo = hi.saturating_sub(1);
                let (a, b) = (st.blend[lo], st.blend[hi]);
                let w = if b.1 > a.1 { ((v - a.1) / (b.1 - a.1)).clamp(0.0, 1.0) } else { 1.0 };
                let moving = play(self, a.0, mix);
                // ponytail: while a transition into a blend state is still mixing, the second
                // animation's weight is approximated as w * mix. Exact once the mix completes.
                play(self, b.0, w * mix) | moving
            }
            _ => false,
        }
    }

    fn apply_binds(&mut self) {
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        for b in &ab.binds {
            let Some(def) = schema::prop(b.prop) else { continue };
            let src = &self.vm[b.slot];
            let v = match def.kind {
                Kind::Float => PVal::F(src.num * b.mul + b.add),
                Kind::Color => PVal::C(src.color),
                Kind::Str if b.prop == p::D => PVal::P(Arc::new(parse_svg_path(&src.text).unwrap_or_default())),
                Kind::Str => PVal::S(src.text.clone()),
                Kind::Uint | Kind::Bool | Kind::Enum(_) => PVal::U((src.num * b.mul + b.add).max(0.0) as u32),
                _ => continue,
            };
            self.set(b.node, b.prop, v);
        }
    }

    pub(crate) fn local_matrix(&self, n: u32) -> Mat {
        Mat::trs(self.f(n, p::X), self.f(n, p::Y), self.f(n, p::ROTATION), self.f(n, p::SCALE_X), self.f(n, p::SCALE_Y))
    }

    fn update_world(&mut self) {
        let file = self.file.clone();
        for (i, n) in file.artboards[self.ab].nodes.iter().enumerate() {
            let parent = if n.parent == NONE { Mat::IDENTITY } else { self.world[n.parent as usize] };
            self.world[i] = if schema::has_transform(n.ty) { parent.mul(&self.local_matrix(i as u32)) } else { parent };
        }
    }

    // ---- data binding ----------------------------------------------------------------

    /// Slot of the property called `name`, if its type is one of `types`
    /// (0 number, 1 bool, 2 trigger, 3 color, 4 string). Accessors are type-checked so a
    /// host cannot silently write a number into a color.
    fn slot(&self, name: &str, types: &[u32]) -> Option<usize> {
        self.def().vm.iter().position(|v| v.name == name && types.contains(&v.ty))
    }
    /// Names and types (`number`, `bool`, `trigger`, `color`, `string`) of the view-model properties.
    pub fn properties(&self) -> Vec<(&str, u32)> {
        self.def().vm.iter().map(|v| (v.name.as_str(), v.ty)).collect()
    }
    pub fn set_number(&mut self, name: &str, v: f32) -> bool {
        let Some(s) = self.slot(name, &[0, 1]).filter(|_| v.is_finite()) else { return false };
        self.dirty |= self.vm[s].num != v;
        self.vm[s].num = v;
        true
    }
    pub fn set_bool(&mut self, name: &str, v: bool) -> bool {
        self.set_number(name, v as u8 as f32)
    }
    pub fn number(&self, name: &str) -> Option<f32> {
        self.slot(name, &[0, 1]).map(|s| self.vm[s].num)
    }
    pub fn set_color(&mut self, name: &str, argb: u32) -> bool {
        let Some(s) = self.slot(name, &[3]) else { return false };
        self.dirty |= self.vm[s].color != argb;
        self.vm[s].color = argb;
        true
    }
    pub fn color(&self, name: &str) -> Option<u32> {
        self.slot(name, &[3]).map(|s| self.vm[s].color)
    }
    pub fn set_string(&mut self, name: &str, v: &str) -> bool {
        let Some(s) = self.slot(name, &[4]) else { return false };
        self.dirty |= &*self.vm[s].text != v;
        self.vm[s].text = v.into();
        true
    }
    pub fn string(&self, name: &str) -> Option<&str> {
        self.slot(name, &[4]).map(|s| &*self.vm[s].text)
    }
    pub fn fire(&mut self, name: &str) -> bool {
        let Some(s) = self.slot(name, &[2]) else { return false };
        self.vm[s].fired = true;
        self.dirty = true;
        true
    }

    /// Names of the events emitted since the last call.
    pub fn take_events(&mut self) -> Vec<String> {
        let file = self.file.clone();
        let mut names: Vec<String> = self.events.drain(..).filter_map(|e| file.objects.get(e as usize)?.str(p::NAME).map(String::from)).collect();
        // Events from nested artboards bubble up to the host.
        for (_, child) in &mut self.nested {
            names.extend(child.take_events());
        }
        names
    }

    // ---- pointer ---------------------------------------------------------------------

    /// Feed a pointer event in surface pixels (the space of the last `render`).
    pub fn pointer(&mut self, kind: Pointer, x: f32, y: f32) {
        // Nested artboards remember their own view from the last render, so they can take
        // the same surface coordinates.
        for (_, child) in &mut self.nested {
            child.pointer(kind, x, y);
            self.dirty |= child.dirty;
        }
        let Player::Machine { idx, .. } = self.player else { return };
        let Some(inv) = self.view.invert() else { return };
        let at = inv.apply(pt(x, y));
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        for (i, l) in ab.machines[idx].listeners.iter().enumerate() {
            let hit = kind != Pointer::Exit
                && match l.target {
                    Some(n) => self.hit_node(n, at),
                    None => at.x >= 0.0 && at.y >= 0.0 && at.x <= self.width() && at.y <= self.height(),
                };
            let fire = match (l.event, kind) {
                (0, _) => hit && !self.hover[i],
                (1, _) => !hit && self.hover[i],
                (2, Pointer::Down) | (5, Pointer::Move) => hit,
                // A release must always be heard, or a press would stick: an `up` listener
                // without a target fires wherever the pointer is released, and when the
                // pointer leaves the surface in the middle of a press.
                (3, Pointer::Up) => hit || l.target.is_none(),
                (3, Pointer::Exit) => l.target.is_none() && self.pressed[i],
                (4, Pointer::Up) => hit && self.pressed[i],
                _ => false,
            };
            self.hover[i] = hit;
            match kind {
                Pointer::Down => self.pressed[i] = hit,
                Pointer::Up | Pointer::Exit => self.pressed[i] = false,
                _ => {}
            }
            if !fire {
                continue;
            }
            if let Some(s) = l.slot {
                let v = &mut self.vm[s];
                match l.action {
                    0 => v.num = l.value,
                    1 => v.num = if v.num != 0.0 { 0.0 } else { 1.0 },
                    2 => v.fired = true,
                    _ => v.num += l.value,
                }
            }
            self.events.extend(l.emit);
            self.dirty = true;
        }
    }

    /// Outline of a Shape / Text / Artboard node in its own space.
    pub(crate) fn contours(&self, n: u32, tol: f32) -> Vec<Contour> {
        let mut out = Vec::new();
        let ab = self.def();
        match ab.nodes[n as usize].ty {
            t::ARTBOARD => {
                let (w, h) = (self.width(), self.height());
                out.push(Contour { pts: vec![pt(0.0, 0.0), pt(w, 0.0), pt(w, h), pt(0.0, h)], closed: true });
            }
            t::TEXT_NODE => geom::flatten(&self.geometry(n), &Mat::IDENTITY, tol, &mut out),
            t::IMAGE | t::MODEL => {
                let (w, h) = self.box_size(n);
                geom::flatten(&geom::rect(w, h, 0.0), &Mat::IDENTITY, tol, &mut out);
            }
            _ => {
                for &g in &ab.nodes[n as usize].children {
                    if schema::is_geometry(ab.nodes[g as usize].ty) {
                        let m = self.local_matrix(g);
                        geom::flatten(&self.geometry(g), &m, tol / m.max_scale().max(1e-6), &mut out);
                    }
                }
            }
        }
        out
    }

    /// Width / height of an Image or Model node (an Image defaults to its pixel size).
    pub(crate) fn box_size(&self, n: u32) -> (f32, f32) {
        let (mut w, mut h) = (self.f(n, p::WIDTH), self.f(n, p::HEIGHT));
        if (w <= 0.0 || h <= 0.0) && self.def().nodes[n as usize].ty == t::IMAGE {
            let asset = self.u(n, p::ASSET);
            if let Some(img) = self.file.images.lock().unwrap_or_else(|e| e.into_inner()).get(&asset) {
                (w, h) = (img.w as f32, img.h as f32);
            } else {
                let a = self.file.objects.get(asset as usize);
                (w, h) = (a.and_then(|a| a.float(p::WIDTH)).unwrap_or(0.0), a.and_then(|a| a.float(p::HEIGHT)).unwrap_or(0.0));
            }
        }
        (w, h)
    }

    /// Local outline of a geometry or text node (cached by `render`).
    pub(crate) fn geometry(&self, n: u32) -> Arc<Path> {
        if let Some(g) = &self.geo[n as usize] {
            return g.clone();
        }
        let (w, h) = (self.f(n, p::WIDTH), self.f(n, p::HEIGHT));
        Arc::new(match self.def().nodes[n as usize].ty {
            t::RECTANGLE => geom::rect(w, h, self.f(n, p::RADIUS)),
            t::ELLIPSE => geom::ellipse(w, h),
            t::POLYGON => geom::star(self.u(n, p::POINTS), w, h, 1.0, false),
            t::STAR => geom::star(self.u(n, p::POINTS), w, h, self.f(n, p::INNER_RADIUS), true),
            t::PATH => match self.get(n, p::D) {
                Some(PVal::P(path)) => return path.clone(),
                _ => Path::new(),
            },
            t::TEXT_NODE => self.text_outline(n),
            _ => Path::new(),
        })
    }

    #[cfg(feature = "text")]
    fn text_outline(&self, n: u32) -> Path {
        let font = self.file.objects.get(self.u(n, p::ASSET) as usize).and_then(|o| o.bytes(p::BYTES)).unwrap_or(&[]);
        crate::text::layout(
            font,
            self.s(n, p::TEXT),
            self.f(n, p::SIZE),
            self.f(n, p::WIDTH),
            self.u(n, p::ALIGN),
            self.f(n, p::LINE_HEIGHT),
            self.f(n, p::WEIGHT),
        )
    }
    #[cfg(not(feature = "text"))]
    fn text_outline(&self, _n: u32) -> Path {
        Path::new()
    }

    /// Does artboard-space point `at` land on the outline of drawable node `n` itself?
    fn hits(&self, n: u32, at: P) -> bool {
        matches!(self.def().nodes[n as usize].ty, t::SHAPE | t::TEXT_NODE | t::IMAGE | t::MODEL)
            && self.world[n as usize].invert().is_some_and(|inv| geom::contains(&self.contours(n, 0.25), inv.apply(at)))
    }

    /// Does artboard-space point `at` land on node `n` or anything inside it?
    fn hit_node(&self, n: u32, at: P) -> bool {
        let nodes = &self.def().nodes;
        self.hits(n, at)
            || nodes[n as usize]
                .children
                .iter()
                .any(|c| matches!(nodes[*c as usize].ty, t::NODE | t::SHAPE | t::TEXT_NODE | t::IMAGE | t::MODEL) && self.hit_node(*c, at))
    }

    /// File index of the top-most drawable under surface pixel `x, y` (editor picking).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<u32> {
        let at = self.view.invert()?.apply(pt(x, y));
        (1..self.def().nodes.len() as u32).rev().find(|n| self.hits(*n, at)).map(|n| self.def().nodes[n as usize].obj)
    }

    /// Artboard-space bounding box `[x0, y0, x1, y1]` of an object and its descendants.
    pub fn bounds(&self, obj: u32) -> Option<[f32; 4]> {
        let n = *self.def().local.get(&obj)?;
        let mut all = Vec::new();
        let mut stack = vec![n];
        while let Some(n) = stack.pop() {
            let node = &self.def().nodes[n as usize];
            if matches!(node.ty, t::ARTBOARD | t::SHAPE | t::TEXT_NODE | t::IMAGE | t::MODEL) {
                let mut cs = self.contours(n, 0.25);
                geom::transform_contours(&mut cs, &self.world[n as usize]);
                all.extend(cs);
            } else if schema::is_geometry(node.ty) {
                let mut cs = Vec::new();
                geom::flatten(&self.geometry(n), &self.world[n as usize], 0.25, &mut cs);
                all.extend(cs);
            }
            stack.extend(node.children.iter().filter(|c| schema::has_transform(self.def().nodes[**c as usize].ty)));
        }
        geom::bounds(&all)
    }

    /// Node -> artboard transform of an object (editor handles).
    pub fn world_matrix(&self, obj: u32) -> Option<Mat> {
        self.def().local.get(&obj).map(|n| self.world[*n as usize])
    }

    // ---- rendering -------------------------------------------------------------------

    /// The transform that fits this artboard into a `w` x `h` surface.
    pub fn fit(&self, fit: Fit, w: f32, h: f32) -> Mat {
        let (aw, ah) = (self.width(), self.height());
        if aw <= 0.0 || ah <= 0.0 {
            return Mat::IDENTITY;
        }
        let (sx, sy) = match fit {
            Fit::Contain => ((w / aw).min(h / ah), (w / aw).min(h / ah)),
            Fit::Cover => ((w / aw).max(h / ah), (w / aw).max(h / ah)),
            Fit::Fill => (w / aw, h / ah),
            Fit::None => (1.0, 1.0),
        };
        Mat { a: sx, b: 0.0, c: 0.0, d: sy, tx: (w - aw * sx) * 0.5, ty: (h - ah * sy) * 0.5 }
    }

    /// Draw into a premultiplied RGBA8 surface of `w * h * 4` bytes. `view` maps artboard
    /// units to pixels (see `fit`). The surface is drawn over, not cleared.
    pub fn render(&mut self, raster: &mut Raster, px: &mut [u8], w: u32, h: u32, view: Mat) {
        if px.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        raster.begin(w, h);
        self.requests.clear();
        self.draw(raster, px, view, 1.0);
    }

    /// Shader paints seen by the last `render`, with the pixel size each needs. The host
    /// renders each shader to RGBA and hands it back through `set_shader_texture`.
    pub fn shader_requests(&self) -> &[ShaderRequest] {
        &self.requests
    }

    /// Supply the pixels (straight-alpha RGBA8) for a ShaderPaint. Used from the next render.
    pub fn set_shader_texture(&mut self, paint: u32, w: u32, h: u32, rgba: &[u8]) -> bool {
        if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
            return false;
        }
        let mut px = rgba.to_vec();
        crate::raster::premultiply(&mut px);
        self.textures.insert(paint, Arc::new(Image { w, h, px }));
        self.dirty = true;
        true
    }
}

/// Timeline position for an elapsed frame count, and whether playback has ended.
fn map_frame(a: &Anim, frames: f64) -> (f32, bool) {
    let d = a.duration as f64;
    if d <= 0.0 {
        return (0.0, true);
    }
    let frames = frames.max(0.0);
    let (frame, finished) = match a.looping {
        0 => (frames.min(d), frames >= d),
        1 => (frames % d, false),
        _ => {
            let m = frames % (2.0 * d);
            (if m > d { 2.0 * d - m } else { m }, false)
        }
    };
    (frame as f32, finished)
}

fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        ((x + (y - x) * t + 0.5) as u32).min(255) << shift
    };
    ch(24) | ch(16) | ch(8) | ch(0)
}

/// CSS-style cubic-bezier easing: find the curve parameter where x == `t`, return y.
fn cubic_ease([x1, y1, x2, y2]: [f32; 4], t: f32) -> f32 {
    let bez = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0));
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if bez(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, (lo + hi) * 0.5)
}

/// Value of a track at `frame`.
fn sample(tr: &Track, frame: f32) -> Option<PVal> {
    let (first, last) = (tr.keys.first()?, tr.keys.last()?);
    if frame <= first.frame {
        return Some(first.val.clone());
    }
    if frame >= last.frame {
        return Some(last.val.clone());
    }
    let hi = tr.keys.partition_point(|k| k.frame <= frame);
    let (k0, k1) = (&tr.keys[hi - 1], &tr.keys[hi]);
    let t = (frame - k0.frame) / (k1.frame - k0.frame);
    let t = match k0.interp {
        1 => return Some(k0.val.clone()),
        2 => cubic_ease(k0.ease, t),
        _ => t,
    };
    Some(match (&k0.val, &k1.val) {
        (PVal::F(a), PVal::F(b)) => PVal::F(a + (b - a) * t),
        (PVal::C(a), PVal::C(b)) => PVal::C(lerp_color(*a, *b, t.clamp(0.0, 1.0))),
        (PVal::P(a), PVal::P(b)) => PVal::P(Arc::new(lerp_path(a, b, t))),
        (a, _) => a.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints_and_shape() {
        let ease = [0.42, 0.0, 0.58, 1.0];
        assert!(cubic_ease(ease, 0.0).abs() < 1e-3);
        assert!((cubic_ease(ease, 1.0) - 1.0).abs() < 1e-3);
        assert!((cubic_ease(ease, 0.5) - 0.5).abs() < 1e-3);
        assert!(cubic_ease(ease, 0.25) < 0.25); // ease-in-out starts slow
        assert!((cubic_ease([0.0, 0.0, 1.0, 1.0], 0.3) - 0.3).abs() < 1e-3); // linear
    }

    #[test]
    fn color_lerp_is_per_channel() {
        assert_eq!(lerp_color(0xff000000, 0xffffffff, 0.5), 0xff808080);
        assert_eq!(lerp_color(0x00102030, 0xff102030, 1.0), 0xff102030);
    }
}
