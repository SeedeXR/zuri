//! A playable instance of one artboard.
//!
//! Per frame the host calls `advance(dt)` (state machine -> animations -> data binds ->
//! transforms) and, if that returned `true`, `render(..)`. `false` means nothing changed
//! and the previous frame is still valid, so idle scenes cost nothing.

use crate::file::{Act, Anim, Artboard, File, Image, Layer, Listener, PVal, Play, State, Track, EVENT_KEY_DOWN, EVENT_KEY_UP, NONE};
use crate::format::Value;
use crate::geom::{self, lerp_path, parse_svg_path, pt, Contour, Mat, Path, P};
use crate::schema::{self, p, t, Kind};
use crate::{Raster, Result};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// How an artboard is scaled into a surface (always centered).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    Contain,
    Cover,
    Fill,
    None,
    /// Scale so the widths match; the height may overflow or leave room.
    FitWidth,
    FitHeight,
    /// `Contain` when the artboard is larger than the surface, its own size otherwise.
    ScaleDown,
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
    /// Fired by a Transition's Action: becomes `fired` for the whole of the next frame.
    pending: bool,
}
/// Everything a Property holds, to tell whether it changed.
type Held = (f32, u32, Arc<str>);

impl VmVal {
    fn value(&self) -> Held {
        (self.num, self.color, self.text.clone())
    }
}

#[derive(Clone, Default)]
struct LayerRt {
    cur: usize,
    time: f64,
    /// The state a cross-fade is leaving, still playing underneath. None when the fade
    /// interrupted another one: it then starts from the frozen picture in `base`.
    from: Option<(usize, f64)>,
    mix: f32,
    /// Length of the running cross-fade in seconds; 0 when none is running.
    mix_dur: f32,
    /// What the properties in play held when the cross-fade's transition fired.
    base: Vec<(u32, u16, PVal)>,
    /// The cross-fade's curve (`Transition::interp` and `ease`) and whether `from` is frozen.
    interp: u32,
    ease: [f32; 4],
    pause: bool,
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

/// Where the generator behind `Transition::chance` starts.
const SEED: u32 = 0x9e37_79b9;

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
    /// Per Skin of the artboard: from its Path's space to its Bone's, in the bind pose
    /// (None for a bone with no size there: it holds nothing).
    bind: Vec<Option<Mat>>,
    /// Per Text set along an outline: a fingerprint of what it stood on when laid out.
    bent_at: Vec<u64>,
    /// Per node, when the artboard has Layouts: where its Layout puts it (added to its
    /// own x / y), and for a Layout the size it came to.
    slots: Vec<(f32, f32)>,
    sizes: Vec<(f32, f32)>,
    /// Per node, when the artboard has Layouts: the box of a Text's lines.
    lines: Vec<[f32; 4]>,
    vm: Vec<VmVal>,
    player: Player,
    /// (NestedArtboard node, its instance), parallel to `Artboard::nests`.
    pub(crate) nested: Vec<(u32, Instance)>,
    events: Vec<u32>,
    /// Artboard -> surface pixels, from the last render. Pointer input is mapped through it.
    pub(crate) view: Mat,
    dirty: bool,
    hover: Vec<bool>,
    pressed: Vec<bool>,
    /// What each `change` Listener last saw in the Property it watches (one per listener).
    seen: Vec<Option<Held>>,
    /// What each two-way Bind last wrote, to tell which side changed (one per bind).
    synced: Vec<Option<PVal>>,
    /// The text each Bind onto a string property last built, and the value it was built from.
    texts: Vec<Option<(Held, Arc<str>)>>,
    pub(crate) time: f64,
    /// `File::image_gen` as last seen: a late `set_image` must redraw a settled scene.
    image_gen: u32,
    pub(crate) textures: HashMap<u32, Arc<Image>>,
    pub(crate) requests: Vec<ShaderRequest>,
    has_shader: bool,
    /// State of the generator behind `Transition::chance`. Every instance starts from the
    /// same seed, so a given sequence of calls always plays out the same way.
    rng: u32,
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
                list.extend(pval(*key, raw).map(|v| (*key, v)));
            }
            props.push(list);
        }
        let mut inst = Instance {
            file: file.clone(),
            ab: artboard,
            props,
            world: vec![Mat::IDENTITY; ab.nodes.len()],
            geo: vec![None; ab.nodes.len()],
            bind: Vec::new(),
            bent_at: Vec::new(),
            slots: Vec::new(),
            sizes: Vec::new(),
            lines: Vec::new(),
            vm: ab.vm.iter().map(|v| VmVal { num: v.num, color: v.color, text: v.text.clone(), fired: false, pending: false }).collect(),
            player: Player::None,
            nested: Vec::new(),
            events: Vec::new(),
            view: Mat::IDENTITY,
            dirty: true,
            hover: Vec::new(),
            pressed: Vec::new(),
            seen: Vec::new(),
            synced: ab.binds.iter().map(|_| None).collect(),
            texts: ab.binds.iter().map(|_| None).collect(),
            time: 0.0,
            image_gen: 0,
            textures: HashMap::new(),
            requests: Vec::new(),
            has_shader: ab.nodes.iter().any(|n| n.ty == t::SHADER_PAINT),
            rng: SEED,
        };
        // ponytail: nesting is capped at 8 levels, which also stops an artboard that nests itself;
        // `budget` caps the total, because 8 levels of 8 children would be 16 million instances.
        if depth < 8 {
            for nest in &ab.nests {
                if *budget == 0 {
                    bail!("more than {MAX_NESTED} nested artboard instances (does an artboard nest itself?)");
                }
                *budget -= 1;
                let mut child = Instance::with_depth(file, nest.artboard, depth + 1, budget)?;
                child.start(nest.play);
                inst.has_shader |= child.has_shader;
                inst.nested.push((nest.node, child));
            }
        }
        // A skinned Path fits its bones in the pose the document's own values describe:
        // before Constraints, animation and Binds.
        if !ab.skins.is_empty() {
            // Where Layouts put things is part of that pose.
            if !ab.layouts.is_empty() {
                inst.lay_out(ab);
            }
            for i in 0..ab.nodes.len() {
                inst.world[i] = inst.placed(ab, i, &[]);
            }
            inst.bind = ab.skins.iter().map(|s| Some(inst.world[s.bone as usize].invert()?.mul(&inst.world[s.path as usize]))).collect();
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
        if !matches!(key, p::X | p::Y | p::ROTATION | p::SCALE_X | p::SCALE_Y | p::OPACITY | p::ORDER | p::SOLO) {
            self.geo[n as usize] = None;
            // A Text and its Runs are laid out together.
            let ab = &self.file.artboards[self.ab];
            let text = if ab.nodes[n as usize].ty == t::RUN { ab.nodes[n as usize].parent } else { n };
            self.geo[text as usize] = None;
            for run in ab.runs(text) {
                self.geo[run as usize] = None;
            }
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
        let seen: Vec<_> = m.listeners.iter().map(|l| l.watch.map(|s| self.vm[s].value())).collect();
        self.player = Player::Machine { idx, layers };
        self.rng = SEED; // a machine that starts over draws the same `chance` sequence again
        self.hover = vec![false; seen.len()];
        self.pressed = vec![false; seen.len()];
        self.seen = seen;
        self.dirty = true;
        true
    }

    /// Play the first state machine, or failing that the first animation.
    pub fn autoplay(&mut self) {
        if !self.play_state_machine(0) {
            self.play_animation(0);
        }
    }

    fn start(&mut self, play: Play) {
        match play {
            Play::Auto => self.autoplay(),
            Play::Machine(idx) => drop(self.play_state_machine(idx)),
            Play::Anim(idx) => drop(self.play_animation(idx)),
        }
    }

    /// Jump the directly played animation to `seconds` on its timeline; the next `advance`
    /// shows it. Nested artboards restart and run to the same time. False when no animation
    /// is playing (a state machine has no one timeline).
    pub fn seek(&mut self, seconds: f32) -> bool {
        let seconds = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 };
        let Player::Anim { time, .. } = &mut self.player else { return false };
        *time = seconds as f64;
        // Nested artboards run on their own clocks: restart them and run them to the same
        // time, so a scrubbed or exported frame does not depend on what was shown before.
        // ponytail: one step of `seconds`. A nested state machine that needs several frames
        // to settle (chained transitions, exit times) can differ from real playback.
        let file = self.file.clone();
        for (nest, (_, child)) in file.artboards[self.ab].nests.iter().zip(&mut self.nested) {
            child.start(nest.play);
            child.advance(seconds);
            child.take_events(); // a seek is a jump: nothing on the way is reported
        }
        self.dirty = true;
        true
    }

    /// The State (file index) each layer of the playing state machine is in; `NONE` for a
    /// layer without states. Empty when no state machine is playing.
    pub fn states(&self) -> Vec<u32> {
        let Player::Machine { idx, layers } = &self.player else { return Vec::new() };
        self.def().machines[*idx].layers.iter().zip(layers).map(|(l, rt)| l.states.get(rt.cur).map_or(NONE, |s| s.obj)).collect()
    }

    /// The value scene object `obj` (file index) currently draws with for a number, bool,
    /// enum or count property: what animation and binds left there, or the default.
    pub fn property(&self, obj: u32, key: u16) -> Option<f32> {
        match self.carried(obj, key)? {
            (n, Kind::Float) => Some(self.f(n, key)),
            (n, Kind::Uint | Kind::Bool | Kind::Enum(_)) => Some(self.u(n, key) as f32),
            _ => None,
        }
    }

    /// Like `property`, for color properties (`0xAARRGGBB`).
    pub fn property_color(&self, obj: u32, key: u16) -> Option<u32> {
        match self.carried(obj, key)? {
            (n, Kind::Color) => Some(self.c(n, key)),
            _ => None,
        }
    }

    /// Node index of `obj` and the kind of `key`, if the object's type has that property.
    fn carried(&self, obj: u32, key: u16) -> Option<(u32, Kind)> {
        let n = *self.def().local.get(&obj)?;
        schema::type_def(self.def().nodes[n as usize].ty)?.has(key).then_some((n, schema::prop(key)?.kind))
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
                let before = time;
                let time = time + (dt * a.speed) as f64;
                let (frame, finished) = map_frame(a, time * a.fps as f64);
                self.apply(a, frame, 1.0);
                cues(a, before * a.fps as f64, time * a.fps as f64, &mut self.events);
                self.player = Player::Anim { idx, time };
                !finished
            }
            Player::Machine { idx, mut layers } => {
                self.hear_changes(idx);
                let active = self.advance_machine(idx, &mut layers, dt);
                self.player = Player::Machine { idx, layers };
                active
            }
        };
        // Data goes down into nested artboards before they advance; a trigger fired here
        // fires there in the same frame.
        let file = self.file.clone();
        for (nest, (_, child)) in file.artboards[self.ab].nests.iter().zip(&mut self.nested) {
            for &(from, to) in &nest.binds {
                let (src, dst) = (&self.vm[from], &mut child.vm[to]);
                if dst.num != src.num || dst.color != src.color || dst.text != src.text || src.fired {
                    (dst.num, dst.color, dst.text, dst.fired) = (src.num, src.color, src.text.clone(), dst.fired || src.fired);
                    child.dirty = true;
                }
            }
        }
        for v in &mut self.vm {
            v.fired = std::mem::take(&mut v.pending);
        }
        self.apply_binds();
        for (_, child) in &mut self.nested {
            active |= child.advance(dt);
        }
        // Transforms follow from properties: nothing was set, nothing moved.
        if self.dirty {
            self.update_world();
        }
        // Shaders run on time, so a scene with one keeps redrawing, but only for hosts that
        // actually draw shaders (they have supplied a texture). Others settle normally.
        std::mem::take(&mut self.dirty) || active || (self.has_shader && !self.textures.is_empty())
    }

    fn apply(&mut self, a: &Anim, frame: f32, mix: f32) {
        for tr in &a.tracks {
            let Some(mut v) = sample(tr, frame) else { continue };
            if mix < 1.0 {
                v = mixed(self.get(tr.node, tr.prop), v, mix, tr.prop);
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
                let mut ready = candidates.chain(&layer.states[rt.cur].transitions).filter(|tr| {
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
                            let value = c.with.map_or(c.value, |s| self.vm[s].num);
                            match c.op {
                                0 => v.num == value,
                                1 => v.num != value,
                                2 => v.num < value,
                                3 => v.num <= value,
                                4 => v.num > value,
                                _ => v.num >= value,
                            }
                        })
                });
                let Some(mut tr) = ready.next() else { break };
                if tr.chance > 0.0 {
                    // A draw among the transitions that could fire now and take part in it:
                    // each further one replaces the pick with probability chance / total so far.
                    let mut total = tr.chance;
                    for t in ready.filter(|t| t.chance > 0.0) {
                        total += t.chance;
                        // xorshift32
                        self.rng ^= self.rng << 13;
                        self.rng ^= self.rng >> 17;
                        self.rng ^= self.rng << 5;
                        if (self.rng >> 8) as f32 / (1u32 << 24) as f32 * total < t.chance {
                            tr = t;
                        }
                    }
                }
                spent.extend(tr.conds.iter().map(|c| c.slot).filter(|s| ab.vm[*s].ty == 2));
                self.events.extend(tr.emit);
                self.act(&tr.acts, true);
                // What a fade starts from. Normally the state being left keeps playing
                // underneath. A fade that interrupts one already on screen starts from the
                // picture as it is: two states are mixed in it and neither alone continues
                // it, so nothing plays underneath, and what the new state does not animate
                // stays where it is. A fade that fired earlier in this same frame has mixed
                // nothing yet: the state on screen is still `rt.from`, so only the target moves.
                let fading = rt.mix_dur > 0.0;
                if tr.blend <= 0.0 {
                    (rt.from, rt.base) = (None, Vec::new());
                } else if fading && rt.mix == 0.0 {
                    rt.base.extend(self.pose(ab, &layer.states[tr.to]));
                } else {
                    rt.from = (!fading).then_some((rt.cur, rt.time));
                    rt.base = self.pose(ab, &layer.states[tr.to]);
                }
                (rt.cur, rt.time, rt.mix, rt.mix_dur) = (tr.to, 0.0, 0.0, tr.blend);
                (rt.interp, rt.ease, rt.pause) = (tr.interp, tr.ease, tr.pause);
                self.dirty = true;
            }
            let st = &layer.states[rt.cur];
            let before = rt.time;
            rt.time += (dt * layer.states[rt.cur].speed) as f64;
            // Only the current state reports cues (one being faded out is silent), and in a
            // blend state only the animations that have weight.
            let (first, second) = self.playing(st);
            let heard = first.filter(|_| second.is_none_or(|s| s.1 < 1.0)).into_iter().chain(second.filter(|s| s.1 > 0.0).map(|s| s.0));
            let direct = st.blend.iter().filter(|d| st.kind == 5 && weight(&self.vm, d) > 0.0).map(|d| d.0);
            for a in heard.chain(direct).map(|a| &ab.anims[a]) {
                let rate = (a.fps * a.speed) as f64;
                cues(a, before * rate, rt.time * rate, &mut self.events);
            }
            if rt.mix_dur > 0.0 {
                rt.mix += dt / rt.mix_dur;
                if rt.mix >= 1.0 {
                    (rt.from, rt.mix_dur) = (None, 0.0);
                    rt.base.clear();
                }
            }
            if rt.mix_dur > 0.0 {
                // Mix from the pose the transition started at, not from last frame's mix:
                // otherwise a property the outgoing state does not animate compounds.
                for (n, key, v) in &rt.base {
                    self.set(*n, *key, v.clone());
                }
                if let Some((from, time)) = &mut rt.from {
                    if !rt.pause {
                        *time += (dt * layer.states[*from].speed) as f64;
                    }
                    self.apply_state(ab, layer, *from, *time, 1.0);
                }
                let mix = match rt.interp {
                    1 => 0.0,                                         // hold: the new state appears when the fade ends
                    2 => cubic_ease(rt.ease, rt.mix).clamp(0.0, 1.0), // an overshooting curve cannot over-mix
                    _ => rt.mix,
                };
                self.apply_state(ab, layer, rt.cur, rt.time, mix);
                active = true;
            } else {
                active |= self.apply_state(ab, layer, rt.cur, rt.time, 1.0);
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
        if st.kind == 5 {
            // A `direct` state is a pose like any other: the design values, then each
            // animation over them at its own weight, in document order. It is worked out
            // in place, so the frame only counts as changed if the result differs.
            let (before, was_dirty) = (self.pose(ab, st), self.dirty);
            for (n, key, _) in &before {
                let raw = self.file.objects[ab.nodes[*n as usize].obj as usize].get(*key);
                let design = match (raw.and_then(|raw| pval(*key, raw)), schema::prop(*key).map(|d| d.kind)) {
                    (Some(v), _) => v,
                    (None, Some(Kind::Color)) => PVal::C(schema::DEFAULT_COLOR),
                    _ => PVal::F(schema::default(*key)),
                };
                self.set(*n, *key, design);
            }
            let mut moving = false;
            for d in &st.blend {
                let w = weight(&self.vm, d);
                moving |= play(self, d.0, w);
            }
            let after: Vec<PVal> =
                before.iter().map(|(n, key, old)| mixed(Some(old), self.get(*n, *key).cloned().unwrap_or_else(|| old.clone()), mix, *key)).collect();
            let changed = before.iter().zip(&after).any(|(b, a)| b.2 != *a);
            for ((n, key, _), v) in before.into_iter().zip(after) {
                self.set(n, key, v);
            }
            self.dirty = was_dirty || changed;
            return moving;
        }
        let (first, second) = self.playing(st);
        let moving = first.is_some_and(|a| play(self, a, mix));
        // ponytail: while a transition into a blend state is still mixing, the second
        // animation's weight is approximated as w * mix. Exact once the mix completes.
        second.is_some_and(|(b, w)| play(self, b, w * mix)) | moving
    }

    /// Run the `change` Listeners whose Property has a new value since they last looked
    /// (or, for a trigger, fired). Once a frame, before the layers, so they see the result.
    fn hear_changes(&mut self, idx: usize) {
        let file = self.file.clone();
        for (i, l) in file.artboards[self.ab].machines[idx].listeners.iter().enumerate() {
            let Some(slot) = l.watch else { continue };
            let now = Some(self.vm[slot].value());
            if self.vm[slot].fired || now != self.seen[i] {
                self.seen[i] = now;
                // A trigger fired from here fires for the whole next frame, so that every
                // Listener and layer hears it whatever the order they are written in.
                self.act(&l.acts, true);
                self.events.extend(l.emit);
            }
        }
    }

    /// Carry out the changes of a Transition or Listener that fired. A trigger fires for
    /// one whole frame, so that every layer sees it: this one if a Listener fired it (it
    /// runs between frames), the next one if a Transition did (`next_frame`).
    fn act(&mut self, acts: &[Act], next_frame: bool) {
        for a in acts {
            let v = &mut self.vm[a.slot];
            match a.action {
                0 => v.num = a.value,
                1 => v.num = if v.num != 0.0 { 0.0 } else { 1.0 },
                2 if next_frame => v.pending = true,
                2 => v.fired = true,
                _ => v.num += a.value,
            }
            // An enum stays on one of its Options whatever was added to it.
            let options = self.def().vm[a.slot].options.len();
            if options > 0 {
                self.vm[a.slot].num = geom::within(self.vm[a.slot].num.round(), 0.0, (options - 1) as f32);
            }
        }
    }

    /// The animations a state plays: one in full, and for a blend state a second over it
    /// at a weight (the two that bracket the input's value).
    fn playing(&self, st: &State) -> (Option<usize>, Option<(usize, f32)>) {
        match st.kind {
            0 => (st.anim, None),
            4 if !st.blend.is_empty() => {
                let v = st.input.map_or(0.0, |s| self.vm[s].num);
                let hi = st.blend.iter().position(|b| b.1 >= v).unwrap_or(st.blend.len() - 1);
                let (a, b) = (st.blend[hi.saturating_sub(1)], st.blend[hi]);
                let w = if b.1 > a.1 { ((v - a.1) / (b.1 - a.1)).clamp(0.0, 1.0) } else { 1.0 };
                (Some(a.0), Some((b.0, w)))
            }
            _ => (None, None),
        }
    }

    fn apply_binds(&mut self) {
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        for (i, b) in ab.binds.iter().enumerate() {
            let Some(def) = schema::prop(b.prop) else { continue };
            let (src, from) = (&self.vm[b.slot], &ab.vm[b.slot]);
            // Only a bind to the object applies mul, add and format.
            let (scale, shift) = if b.dir == 0 { (b.mul, b.add) } else { (1.0, 0.0) };
            let v = match def.kind {
                Kind::Float => PVal::F(src.num * scale + shift),
                Kind::Color => PVal::C(src.color),
                Kind::Str if b.prop == p::D => PVal::P(Arc::new(parse_svg_path(&src.text).unwrap_or_default())),
                // The text is built when the Property changes, not every frame.
                Kind::Str => match &self.texts[i] {
                    Some((of, text)) if *of == src.value() => PVal::S(text.clone()),
                    _ => {
                        let text = match from.ty {
                            0 | 1 => fixed(src.num * scale + shift, b.decimals).into(),
                            5 => from.options.get(src.num as usize).cloned().unwrap_or_default(),
                            _ => src.text.clone(),
                        };
                        // `{}` is found and filled in as bytes: `str::replacen`, and slicing a
                        // str at all, bring 10 KB of searching and error formatting into the wasm.
                        let format = b.format.as_deref().filter(|_| b.dir == 0).map_or(&[][..], str::as_bytes);
                        let text = match format.windows(2).position(|w| w == b"{}") {
                            Some(at) => String::from_utf8([&format[..at], text.as_bytes(), &format[at + 2..]].concat()).unwrap_or_default().into(),
                            None => text, // no format, or one without a hole
                        };
                        self.texts[i] = Some((src.value(), text.clone()));
                        PVal::S(text)
                    }
                },
                Kind::Uint | Kind::Bool | Kind::Enum(_) => PVal::U((src.num * scale + shift).max(0.0) as u32),
                _ => continue,
            };
            // The other way: what the object holds goes into the Property.
            let to_source = |this: &mut Instance| {
                let slot = &mut this.vm[b.slot];
                match this.props[b.node as usize].iter().find(|(k, _)| *k == b.prop).map(|(_, v)| v) {
                    Some(PVal::F(x)) => slot.num = *x,
                    Some(PVal::U(x)) => slot.num = *x as f32,
                    Some(PVal::C(x)) => slot.color = *x,
                    Some(PVal::S(x)) => slot.text = x.clone(),
                    // Nothing was ever set there: the property is at its default.
                    None if def.kind == Kind::Color => slot.color = schema::DEFAULT_COLOR,
                    None if def.kind == Kind::Str => slot.text = "".into(),
                    None => slot.num = schema::default(b.prop),
                    _ => {}
                }
            };
            match b.dir {
                0 => self.set(b.node, b.prop, v),
                1 => to_source(self),
                // Two-way: the side that changed since the last frame wins, the Property first.
                _ if self.synced[i].as_ref() != Some(&v) => {
                    self.set(b.node, b.prop, v.clone());
                    self.synced[i] = Some(v);
                }
                _ => {
                    to_source(self);
                    self.synced[i] = self.get(b.node, b.prop).cloned();
                }
            }
        }
    }

    pub(crate) fn local_matrix(&self, n: u32) -> Mat {
        Mat::trs(self.f(n, p::X), self.f(n, p::Y), self.f(n, p::ROTATION), self.f(n, p::SCALE_X), self.f(n, p::SCALE_Y))
    }

    fn update_world(&mut self) {
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        if !ab.layouts.is_empty() {
            self.lay_out(ab);
        }
        // ponytail: with constraints, a second pass applies them in document order. A target
        // that comes later in the document is read as the first pass left it (before its own
        // constraints, and before its parents' moved it); sort by dependency if that matters.
        // A skinned Path's outline follows its bones: Constraints that read it in the second
        // pass, and the next draw, work it out afresh.
        for skin in &ab.skins {
            self.geo[skin.path as usize] = None;
        }
        // What `ik` Constraints turn each bone by, in degrees (only the second pass has any).
        let mut turned = if ab.constrained { vec![0.0; ab.nodes.len()] } else { Vec::new() };
        for pass in 0..=u8::from(ab.constrained) {
            for (i, n) in ab.nodes.iter().enumerate() {
                self.world[i] = self.placed(ab, i, &turned);
                if pass == 1 {
                    for &c in n.children.iter().filter(|c| ab.nodes[**c as usize].ty == t::CONSTRAINT) {
                        match self.u(c, p::CONSTRAIN) {
                            6 => self.reach(ab, c, i, &mut turned),
                            _ => self.world[i] = self.constrained(c, self.world[i]),
                        }
                    }
                }
            }
        }
        // Without the text feature a Text has no outline to refresh.
        if cfg!(feature = "text") {
            self.refresh_bent(ab);
        }
    }

    /// The outlines of `n` and of what is inside it and drawn, through `m`: what a Layout
    /// measures. A Text counts for the box of its lines, whatever its letters are; one
    /// set along a path takes no room (where it is does not depend on where it is put).
    // ponytail: a Bone inside a Bone is taken where its x / y put it, not at its parent's far end.
    fn outlines(&self, ab: &Artboard, n: u32, m: &Mat, out: &mut Vec<Contour>) {
        let node = &ab.nodes[n as usize];
        let mut cs = match node.ty {
            t::TEXT_NODE if self.node_ref(n, p::FOLLOW).is_some() || !cfg!(feature = "text") => Vec::new(),
            t::TEXT_NODE => {
                let b = self.lines[n as usize];
                vec![Contour { pts: vec![pt(b[0], b[1]), pt(b[2], b[1]), pt(b[2], b[3]), pt(b[0], b[3])], closed: true }]
            }
            t::SHAPE | t::IMAGE | t::MODEL | t::LAYOUT => self.contours(n, 0.25),
            t::NESTED_ARTBOARD => self.nested.iter().find(|nest| nest.0 == n).map(|nest| nest.1.contours(0, 0.25)).unwrap_or_default(),
            _ => Vec::new(),
        };
        geom::transform_contours(&mut cs, m);
        out.extend(cs);
        if node.ty != t::LAYOUT {
            for &c in self.drawn_children(ab, n).iter() {
                self.outlines(ab, c, &m.mul(&self.local_matrix(c)), out);
            }
        }
    }

    /// Arrange what is inside every Layout: `sizes` (what each Layout came to) and `slots`
    /// (where each child goes, to which its own x / y are added).
    // ponytail: every Layout is arranged again whenever anything in the artboard changed,
    // and the boxes of its children are measured by flattening their outlines; remember
    // what each depends on if a scene with many Layouts shows this in a profile. A
    // skinned Path is measured as it was last drawn. No wrapping onto further rows, one
    // padding for all sides; only Layouts grow or stretch, and only upright ones.
    fn lay_out(&mut self, ab: &Artboard) {
        self.sizes.clear();
        self.sizes.resize(ab.nodes.len(), (0.0, 0.0));
        self.slots = self.sizes.clone();
        // Texts are measured by their lines: lay out the ones that changed (once: the
        // outlines are kept for drawing).
        self.lines.resize(ab.nodes.len(), [0.0; 4]);
        for n in 0..ab.nodes.len() {
            if cfg!(feature = "text") && ab.nodes[n].ty == t::TEXT_NODE && self.geo[n].is_none() {
                self.lay_text(n as u32);
            }
        }
        // Along the flow and across it: (x, y) for a row, (y, x) for a column.
        let axes = |column: bool, v: (f32, f32)| if column { (v.1, v.0) } else { v };
        // The children that take room, with the box of each as its parent sees it: its
        // own, turned and scaled as the child is, but not moved by its x / y.
        let boxed = |this: &Instance, c: u32| {
            let mut cs = Vec::new();
            this.outlines(ab, c, &Mat { tx: 0.0, ty: 0.0, ..this.local_matrix(c) }, &mut cs);
            geom::bounds(&cs)
        };
        let mut items = Vec::new();
        // Sizes, from the inside out: a Layout without a width (or height) of its own is
        // as large as what it holds.
        for &l in ab.layouts.iter().rev() {
            let (column, gap, pad) = (self.u(l, p::FLOW) == 1, self.f(l, p::GAP), self.f(l, p::PADDING));
            let (mut held, mut along, mut across) = (Vec::new(), 2.0 * pad, 0.0f32);
            for &c in &ab.nodes[l as usize].draws {
                let Some(b) = boxed(self, c) else { continue };
                let size = axes(column, (b[2] - b[0], b[3] - b[1]));
                (along, across) = (along + size.0 + if held.is_empty() { 0.0 } else { gap }, across.max(size.1));
                held.push((c, b));
            }
            let (own, hug) = ((self.f(l, p::WIDTH), self.f(l, p::HEIGHT)), axes(column, (along, across + 2.0 * pad)));
            self.sizes[l as usize] = (if own.0 > 0.0 { own.0 } else { hug.0.max(0.0) }, if own.1 > 0.0 { own.1 } else { hug.1.max(0.0) });
            items.push(held);
        }
        // Places, from the outside in: a Layout's size is final once its parent has placed it.
        for (&l, held) in ab.layouts.iter().zip(items.iter().rev()) {
            let (column, gap, pad, justify, align) =
                (self.u(l, p::FLOW) == 1, self.f(l, p::GAP), self.f(l, p::PADDING), self.u(l, p::JUSTIFY), self.u(l, p::ALIGN_ITEMS));
            let room = axes(column, (self.sizes[l as usize].0 - 2.0 * pad, self.sizes[l as usize].1 - 2.0 * pad));
            // How much larger a child Layout is in its parent than in itself, along and
            // across the flow: None for one that is not a Layout, or is turned (it keeps its size).
            let scale = |this: &Instance, c: u32| {
                let by = axes(column, (this.f(c, p::SCALE_X).abs(), this.f(c, p::SCALE_Y).abs()));
                (ab.nodes[c as usize].ty == t::LAYOUT && this.f(c, p::ROTATION) == 0.0 && by.0 > 0.0 && by.1 > 0.0).then_some(by)
            };
            // The room to spare along the flow, and the `grow` of the Layouts that share it.
            let (mut spare, mut shares) = (room.0 - gap * held.len().saturating_sub(1) as f32, 0.0);
            for &(c, b) in held {
                spare -= axes(column, (b[2] - b[0], b[3] - b[1])).0;
                if scale(self, c).is_some() {
                    shares += self.f(c, p::GROW).max(0.0);
                }
            }
            let mut share = 0.0;
            if shares > 0.0 {
                (share, spare) = (spare.max(0.0) / shares, spare.min(0.0));
            }
            let between = if justify == 3 && held.len() > 1 { gap + spare.max(0.0) / (held.len() - 1) as f32 } else { gap };
            let mut at = pad + [0.0, spare * 0.5, spare, 0.0][justify.min(3) as usize];
            for &(c, mut b) in held {
                if let Some(by) = scale(self, c) {
                    // Its share of the room to spare, and with `stretch` all of the room across.
                    let mut size = axes(column, self.sizes[c as usize]);
                    size.0 += share * self.f(c, p::GROW).max(0.0) / by.0;
                    if align == 3 {
                        size.1 = (room.1 / by.1).max(0.0);
                    }
                    self.sizes[c as usize] = axes(column, size);
                    b = boxed(self, c).unwrap_or(b);
                }
                let (low, size) = (axes(column, (b[0], b[1])), axes(column, (b[2] - b[0], b[3] - b[1])));
                let across = pad + [0.0, (room.1 - size.1) * 0.5, room.1 - size.1, 0.0][align.min(3) as usize];
                self.slots[c as usize] = axes(column, (at - low.0, across - low.1));
                at += size.0 + between;
            }
        }
    }

    /// Texts set along an outline are laid out again when what they stand on has changed:
    /// where the Text or the Shape is, or the Shape's geometry.
    // ponytail: checked after everything is placed, so a Constraint that follows such a
    // Text's own outline reads the one from the frame before.
    fn refresh_bent(&mut self, ab: &Artboard) {
        for (k, &text) in ab.bent.iter().enumerate() {
            let Some(shape) = self.node_ref(text, p::FOLLOW) else { continue };
            // A fingerprint of the matrices involved; geometry that changed has no cached outline.
            let mut changed = false;
            let mut key = 0xcbf2_9ce4_8422_2325u64;
            for &n in [text, shape].iter().chain(&ab.nodes[shape as usize].children) {
                changed |= schema::is_geometry(ab.nodes[n as usize].ty) && self.geo[n as usize].is_none();
                let m = self.world[n as usize];
                for v in [m.a, m.b, m.c, m.d, m.tx, m.ty] {
                    key = (key ^ u64::from(v.to_bits())).wrapping_mul(0x0100_0000_01b3);
                }
            }
            if changed || self.bent_at.get(k) != Some(&key) {
                self.bent_at.resize(ab.bent.len(), 0);
                self.bent_at[k] = key;
                self.geo[text as usize] = None;
                for run in ab.runs(text) {
                    self.geo[run as usize] = None;
                }
            }
        }
    }

    /// World transform of node `i` from its parent's (which must be up to date): its own
    /// transform, turned by what `ik` asked of it, and for a Bone in a Bone, starting at
    /// the far end of the parent.
    fn placed(&self, ab: &Artboard, i: usize, turned: &[f32]) -> Mat {
        let n = &ab.nodes[i];
        if n.parent == NONE {
            return Mat::IDENTITY;
        }
        let m = self.world[n.parent as usize];
        if !schema::has_transform(n.ty) {
            return m;
        }
        let along = if n.ty == t::BONE && ab.nodes[n.parent as usize].ty == t::BONE { self.f(n.parent, p::LENGTH) } else { 0.0 };
        let (turn, slot) = (turned.get(i).copied().unwrap_or(0.0), self.slots.get(i).copied().unwrap_or_default());
        let i = i as u32;
        m.mul(&Mat::trs(
            self.f(i, p::X) + along + slot.0,
            self.f(i, p::Y) + slot.1,
            self.f(i, p::ROTATION) + turn,
            self.f(i, p::SCALE_X),
            self.f(i, p::SCALE_Y),
        ))
    }

    /// The `ik` Constraint `c` on bone `tip`: turn it and its parent bones (`bones` of them
    /// in all) so that its far end reaches the target, as far as the chain can stretch.
    ///
    /// Each bone in turn, from the first, is solved as a triangle with the rest of the chain
    /// taken as one rigid piece (its start to the chain's far end): the bone's length, the
    /// piece's length and the distance to the target give both angles exactly. The elbow
    /// keeps bending the way it bends in the animated pose. There is nothing to iterate, so
    /// the same pose and target always give the same answer.
    // ponytail: while solving, the chain's bones are placed without the other Constraints
    // on them; those are applied again once the chain is posed. Order by dependency if a
    // chain has to be solved on top of another Constraint's result.
    fn reach(&mut self, ab: &Artboard, c: u32, tip: usize, turned: &mut [f32]) {
        let strength = self.f(c, p::STRENGTH).clamp(0.0, 1.0);
        let Some(target) = self.node_ref(c, p::TARGET).filter(|_| strength > 0.0) else { return };
        let start = |w: Mat| pt(w.tx, w.ty);
        let goal = start(self.world[target as usize]);
        // The chain, first bone first: the bone the Constraint is on and its parent bones.
        let (mut chain, mut up) = (vec![(tip, 0.0)], ab.nodes[tip].parent as usize);
        while chain.len() < self.u(c, p::BONES) as usize && ab.nodes[up].ty == t::BONE {
            chain.push((up, 0.0));
            up = ab.nodes[up].parent as usize;
        }
        chain.reverse();
        let angle = |from: P, to: P| (to.y - from.y).atan2(to.x - from.x).to_degrees();
        let apart = |a: P, b: P| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let short = |degrees: f32| (degrees + 180.0).rem_euclid(360.0) - 180.0;
        for k in 0..chain.len() {
            for &(bone, _) in &chain {
                self.world[bone] = self.placed(ab, bone, turned);
            }
            // This bone's start, the rest's start (or the far end, for the last bone), the far end.
            let bone = chain[k].0;
            let a = start(self.world[bone]);
            let end = self.world[tip].apply(pt(self.f(tip as u32, p::LENGTH), 0.0));
            let b = chain.get(k + 1).map_or(end, |next| start(self.world[next.0]));
            let (la, lb, reach) = (apart(a, b), apart(b, end), apart(a, goal));
            // Bend to the side the chain bends to now (clockwise when it is straight).
            let side = if (b.x - a.x) * (end.y - b.y) - (b.y - a.y) * (end.x - b.x) < 0.0 { -1.0 } else { 1.0 };
            // What this bone points at now is `b`; a bone with no length has nothing to point with.
            if la > 1e-6 && reach > 1e-6 {
                // The far end has to come to the target's distance: the angle between this
                // bone and the line to the target, by the law of cosines. Out of reach the
                // cosine leaves -1..1 and is held there: straight at the target, or straight
                // away from it when the rest is the longer part.
                let off = ((la * la + reach * reach - lb * lb) / (2.0 * la * reach)).clamp(-1.0, 1.0).acos().to_degrees();
                // A bone turns in its parent's space: in a mirrored one, the other way round.
                let space = self.world[ab.nodes[bone].parent as usize];
                let mirror = if space.a * space.d - space.b * space.c < 0.0 { -1.0 } else { 1.0 };
                chain[k].1 = short(angle(a, goal) - side * off - angle(a, b)) * mirror;
                turned[bone] += chain[k].1;
            }
        }
        for &(bone, by) in &chain {
            turned[bone] -= by * (1.0 - strength);
        }
        // Pose everything from the chain's first bone to its last, with the other
        // Constraints that had already been applied there.
        for i in chain[0].0..=tip {
            self.world[i] = self.placed(ab, i, turned);
            for &other in ab.nodes[i].children.iter().take_while(|o| **o != c) {
                if ab.nodes[other as usize].ty == t::CONSTRAINT && self.u(other, p::CONSTRAIN) != 6 {
                    self.world[i] = self.constrained(other, self.world[i]);
                }
            }
        }
    }

    /// World transform `w` of a node after Constraint node `c` has had its say. Each part
    /// moves from where it is toward the target's by `strength`, and nothing else changes:
    /// the transform is never rebuilt from parts, so skew survives and strength 0 is `w`.
    fn constrained(&self, c: u32, w: Mat) -> Mat {
        let strength = self.f(c, p::STRENGTH).clamp(0.0, 1.0);
        let Some(target) = self.node_ref(c, p::TARGET).filter(|_| strength > 0.0) else { return w };
        let tw = self.world[target as usize];
        // (rotation in degrees, x scale, y scale). A mirror image counts as a negative x scale.
        let parts = |m: &Mat| {
            let flip = if m.a * m.d - m.b * m.c < 0.0 { -1.0 } else { 1.0 };
            ((m.b * flip).atan2(m.a * flip).to_degrees(), (m.a * m.a + m.b * m.b).sqrt() * flip, (m.c * m.c + m.d * m.d).sqrt())
        };
        let ((rot, sx, sy), (trot, tsx, tsy)) = (parts(&w), parts(&tw));
        let (mut place, mut turn, mut scale) = (None, None, None);
        // A copy takes one axis or both, and what it takes stays within `min`..`max` (`max`
        // wins if they cross).
        // ponytail: limits are in artboard space, like the copy; parent-relative ones need a
        // `space` property. A rotation is read between -180 and 180, so a limited follower
        // jumps when its target turns through 180; that needs the target's turns counted.
        let (axis, limit) = (self.u(c, p::AXIS), |v: f32| v.max(self.f(c, p::MIN)).min(self.f(c, p::MAX)));
        let take = |other: u32, target: f32, own: f32| if axis == other { own } else { target };
        match self.u(c, p::CONSTRAIN) {
            0 => place = Some((take(2, limit(tw.tx), w.tx), take(1, limit(tw.ty), w.ty))),
            1 => turn = Some(limit(trot)),
            // The limit is on the size: a mirrored target (a negative x scale) stays mirrored.
            2 => scale = Some((take(2, limit(tsx.abs()).copysign(tsx), sx), take(1, limit(tsy), sy))),
            3 => (place, turn, scale) = (Some((tw.tx, tw.ty)), Some(trot), Some((tsx, tsy))),
            4 => {
                let (dx, dy) = (w.tx - tw.tx, w.ty - tw.ty);
                let (len, d) = ((dx * dx + dy * dy).sqrt(), self.f(c, p::DISTANCE).max(0.0));
                let keep = match self.u(c, p::LIMIT) {
                    0 => len.min(d),
                    1 => len.max(d),
                    _ => d,
                };
                // Out along the line from the target; to its right when they coincide.
                place = Some(if len > 0.0 { (tw.tx + dx / len * keep, tw.ty + dy / len * keep) } else { (tw.tx + keep, tw.ty) });
            }
            _ => {
                let mut outline = self.contours(target, 0.25);
                geom::transform_contours(&mut outline, &tw);
                let Some((at, facing)) = geom::point_along(&outline, self.f(c, p::POSITION)) else { return w };
                place = Some((at.x, at.y));
                turn = (self.u(c, p::ORIENT) != 0).then_some(facing);
            }
        }
        let mut m = w;
        if let Some((x, y)) = place {
            (m.tx, m.ty) = (w.tx + (x - w.tx) * strength, w.ty + (y - w.ty) * strength);
        }
        if let Some((tsx, tsy)) = scale {
            // Each axis is stretched toward the target's length, and keeps its direction.
            let by = |own: f32, target: f32| if own != 0.0 { 1.0 + (target / own - 1.0) * strength } else { 1.0 };
            let (kx, ky) = (by(sx, tsx), by(sy, tsy));
            (m.a, m.b, m.c, m.d) = (m.a * kx, m.b * kx, m.c * ky, m.d * ky);
        }
        if let Some(to) = turn {
            // The short way round, as a rotation of what is there.
            let (sin, cos) = (((to - rot + 180.0).rem_euclid(360.0) - 180.0) * strength).to_radians().sin_cos();
            (m.a, m.b, m.c, m.d) = (cos * m.a - sin * m.b, sin * m.a + cos * m.b, cos * m.c - sin * m.d, sin * m.c + cos * m.d);
        }
        m
    }

    // ---- data binding ----------------------------------------------------------------

    /// Slot of the property called `name`, if its type is one of `types`
    /// (0 number, 1 bool, 2 trigger, 3 color, 4 string, 5 enum). Accessors are type-checked so a
    /// host cannot silently write a number into a color.
    fn slot(&self, name: &str, types: &[u32]) -> Option<usize> {
        self.def().vm.iter().position(|v| v.name == name && types.contains(&v.ty))
    }
    /// Names and types (`number`, `bool`, `trigger`, `color`, `string`, `enum`) of the view-model properties.
    pub fn properties(&self) -> Vec<(&str, u32)> {
        self.def().vm.iter().map(|v| (v.name.as_str(), v.ty)).collect()
    }
    /// The names an enum Property can take, in order (its number is a position in this
    /// list). Empty for the other types.
    pub fn options(&self, property: usize) -> &[Arc<str>] {
        self.def().vm.get(property).map_or(&[], |v| &v.options)
    }
    /// Set a number or bool Property, or an enum Property to the Option at that position
    /// (refused when there is no such Option).
    pub fn set_number(&mut self, name: &str, v: f32) -> bool {
        let Some(s) = self.slot(name, &[0, 1, 5]).filter(|_| v.is_finite()) else { return false };
        let options = self.def().vm[s].options.len();
        if self.def().vm[s].ty == 5 && !(v.fract() == 0.0 && v >= 0.0 && (v as usize) < options) {
            return false;
        }
        self.dirty |= self.vm[s].num != v;
        self.vm[s].num = v;
        true
    }
    pub fn set_bool(&mut self, name: &str, v: bool) -> bool {
        self.set_number(name, v as u8 as f32)
    }
    pub fn number(&self, name: &str) -> Option<f32> {
        self.slot(name, &[0, 1, 5]).map(|s| self.vm[s].num)
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
    /// Set a string Property, or an enum Property to the Option of that name.
    pub fn set_string(&mut self, name: &str, v: &str) -> bool {
        if let Some(s) = self.slot(name, &[5]) {
            return self.def().vm[s].options.iter().position(|o| &**o == v).is_some_and(|i| self.set_number(name, i as f32));
        }
        let Some(s) = self.slot(name, &[4]) else { return false };
        self.dirty |= &*self.vm[s].text != v;
        self.vm[s].text = v.into();
        true
    }
    /// A string Property's text, or the name of the Option an enum Property is on.
    pub fn string(&self, name: &str) -> Option<&str> {
        let s = self.slot(name, &[4, 5])?;
        let def = &self.def().vm[s];
        Some(if def.ty == 5 { def.options.get(self.vm[s].num as usize).map_or("", |o| o) } else { &self.vm[s].text })
    }
    pub fn fire(&mut self, name: &str) -> bool {
        let Some(s) = self.slot(name, &[2]) else { return false };
        self.vm[s].fired = true;
        self.dirty = true;
        true
    }

    /// Names of the events `take_events` can report: this artboard's, then those of the
    /// artboards nested in it (their events bubble up).
    pub fn event_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.def().events.iter().map(String::as_str).collect();
        for (_, child) in &self.nested {
            for name in child.event_names() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
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

    // ---- keys and pointer ------------------------------------------------------------

    /// Report a key going down or up, named as the web's `KeyboardEvent.key` names it. True
    /// when a Listener reacted (a host uses that to keep the key from doing anything else).
    pub fn key(&mut self, key: &str, down: bool) -> bool {
        // Nested artboards hear it too, unless a `solo` hides them.
        let mut nested = std::mem::take(&mut self.nested);
        let mut heard = false;
        for (node, child) in &mut nested {
            heard |= self.shown(*node) && child.key(key, down);
        }
        self.nested = nested;
        let Player::Machine { idx, .. } = self.player else { return heard };
        let file = self.file.clone();
        for l in &file.artboards[self.ab].machines[idx].listeners {
            // A key with no name (or one that was not text) is no key: not even "any key" hears it.
            if l.event == if down { EVENT_KEY_DOWN } else { EVENT_KEY_UP } && !key.is_empty() && l.key.as_deref().is_none_or(|k| k == key) {
                self.react(l);
                heard = true;
            }
        }
        heard
    }

    /// Feed a pointer event in surface pixels (the space of the last `render`).
    pub fn pointer(&mut self, kind: Pointer, x: f32, y: f32) {
        // Nested artboards remember their own view from the last render, so they can take
        // the same surface coordinates.
        // One that a `solo` hides is not there to touch: for it the pointer has left.
        let mut nested = std::mem::take(&mut self.nested);
        for (node, child) in &mut nested {
            child.pointer(if self.shown(*node) { kind } else { Pointer::Exit }, x, y);
            self.dirty |= child.dirty;
        }
        self.nested = nested;
        let Player::Machine { idx, .. } = self.player else { return };
        let Some(inv) = self.view.invert() else { return };
        let at = inv.apply(pt(x, y));
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        for (i, l) in ab.machines[idx].listeners.iter().enumerate() {
            let hit = kind != Pointer::Exit
                && match l.target {
                    Some(n) => self.shown(n) && self.hit_node(n, at),
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
            if fire {
                self.react(l);
            }
        }
    }

    /// A Listener heard its pointer event or its key: do what it says.
    fn react(&mut self, l: &Listener) {
        self.act(&l.acts, false);
        self.events.extend(l.emit);
        self.dirty = true;
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
            t::TEXT_NODE | t::RUN => geom::flatten(&self.geometry(n), &Mat::IDENTITY, tol, &mut out),
            // A Layout's box, from its corner, at the size it came to.
            t::LAYOUT => {
                let (w, h) = self.sizes.get(n as usize).copied().unwrap_or_default();
                geom::flatten(&geom::rect(w, h, self.f(n, p::RADIUS)), &Mat::trs(w * 0.5, h * 0.5, 0.0, 1.0, 1.0), tol, &mut out);
            }
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
                Some(PVal::P(path)) => match self.skinned(n, path) {
                    Some(bent) => bent,
                    None => return path.clone(),
                },
                _ => Path::new(),
            },
            t::TEXT_NODE => self.text_outlines(n).0.concat(),
            // Its part of its Text. `draw` lays a Text out once for all of its Runs; this
            // is for a Run asked about before anything was drawn.
            t::RUN => {
                let text = self.def().nodes[n as usize].parent;
                let at = self.def().runs(text).position(|run| run == n);
                self.text_outlines(text).0.into_iter().nth(at.unwrap_or(0)).unwrap_or_default()
            }
            _ => Path::new(),
        })
    }

    /// The outline of Path `n` with each point where its bones carry it from the bind pose:
    /// the mean of the bones' answers, by weight. None when no Skin holds the path.
    fn skinned(&self, n: u32, path: &Path) -> Option<Path> {
        let inv = self.world[n as usize].invert()?;
        // Per Skin: out of the path's space, along with the bone since the bind pose, and back.
        let mut held = Vec::new();
        for (s, bind) in self.def().skins.iter().zip(&self.bind) {
            if let (true, Some(bind)) = (s.path == n, bind) {
                held.push((inv.mul(&self.world[s.bone as usize]).mul(bind), &s.weights[..]));
            }
        }
        if held.is_empty() {
            return None;
        }
        let (mut bent, mut i) = (path.clone(), 0);
        for el in &mut bent {
            for p in el.points().into_iter().flatten() {
                let (mut sum, mut x, mut y) = (0.0, 0.0, 0.0);
                for (m, weights) in &held {
                    // A point that `d` was keyed to add, beyond the weights, is not held.
                    let w = if weights.is_empty() { 1.0 } else { weights.get(i).copied().unwrap_or(0.0) };
                    let q = m.apply(*p);
                    (sum, x, y) = (sum + w, x + q.x * w, y + q.y * w);
                }
                // A point no bone holds stays where the path has it.
                if sum > 0.0 {
                    *p = pt(x / sum, y / sum);
                }
                i += 1;
            }
        }
        Some(bent)
    }

    /// Lay Text `n` out once, for itself and for each of its Runs, and keep the box of its lines.
    pub(crate) fn lay_text(&mut self, n: u32) {
        let (outlines, lines) = self.text_outlines(n);
        let runs: Vec<u32> = self.def().runs(n).collect();
        for (run, outline) in runs.into_iter().zip(&outlines) {
            self.geo[run as usize] = Some(Arc::new(outline.clone()));
        }
        self.geo[n as usize] = Some(Arc::new(outlines.concat()));
        if let Some(kept) = self.lines.get_mut(n as usize) {
            *kept = lines;
        }
    }

    /// The outlines of Text `text`, laid out: one per Run, or just its own without Runs.
    /// And the box of its lines.
    #[cfg(feature = "text")]
    pub(crate) fn text_outlines(&self, text: u32) -> (Vec<Path>, [f32; 4]) {
        let mut runs: Vec<u32> = self.def().runs(text).collect();
        if runs.is_empty() {
            runs.push(text);
        }
        let span = |run: &u32| {
            // A Run that names no font is set in its Text's.
            let asset = [*run, text].map(|n| self.u(n, p::ASSET)).into_iter().find(|a| *a != NONE).unwrap_or(NONE);
            crate::text::Span {
                font: self.file.objects.get(asset as usize).and_then(|o| o.bytes(p::BYTES)).unwrap_or(&[]),
                text: self.s(*run, p::TEXT),
                size: self.f(*run, p::SIZE),
                weight: self.f(*run, p::WEIGHT),
            }
        };
        let spans: Vec<_> = runs.iter().map(span).collect();
        // Flat, unless it follows a Shape (and only a Shape: a file could name anything).
        let Some(shape) = self.node_ref(text, p::FOLLOW).filter(|shape| self.def().nodes[*shape as usize].ty == t::SHAPE) else {
            return crate::text::layout(&spans, self.f(text, p::WIDTH), self.u(text, p::ALIGN), self.f(text, p::LINE_HEIGHT), None);
        };
        // Text on a path: the Shape's outline, seen from where the Text is. With no
        // outline to stand on (a Shape scaled to nothing), there is no text.
        let (outline, length) = self.followed(text, shape).unwrap_or_default();
        // A closed outline goes round and round (so `position` wraps); an open one ends.
        let (position, round) = (self.f(text, p::POSITION), length > 0.0 && outline.iter().all(|c| c.closed));
        let start = if round { position.rem_euclid(1.0) } else { position } * length;
        let along = |d: f32| geom::point_at(&outline, if round { (start + d).rem_euclid(length) } else { start + d }).filter(|_| start + d >= 0.0 || round);
        // No box to wrap in: `align` sets the text from, about or up to `position`.
        crate::text::layout(&spans, 0.0, self.u(text, p::ALIGN), self.f(text, p::LINE_HEIGHT), Some(&along))
    }

    /// The outline of `shape` in the space of Text `text`, and its length. None when
    /// there is none to speak of.
    #[cfg(feature = "text")]
    fn followed(&self, text: u32, shape: u32) -> Option<(Vec<Contour>, f32)> {
        let to_text = self.world[text as usize].invert()?.mul(&self.world[shape as usize]);
        // Flattened finely: a letter turns with the piece it stands on, and pieces as long
        // as a letter would turn it in steps.
        let mut outline = self.contours(shape, 0.01 / to_text.max_scale().max(1e-6));
        geom::transform_contours(&mut outline, &to_text);
        // Seen in a mirror (the Shape or the Text is flipped), the outline runs the other
        // way: walk it backwards, from the same start, so the text stays on its side of it.
        if to_text.a * to_text.d - to_text.b * to_text.c < 0.0 {
            for c in &mut outline {
                c.pts.reverse();
                if c.closed {
                    c.pts.rotate_right(1);
                }
            }
        }
        let length = geom::length(&outline);
        (length > 0.0).then_some((outline, length))
    }
    #[cfg(not(feature = "text"))]
    pub(crate) fn text_outlines(&self, _text: u32) -> (Vec<Path>, [f32; 4]) {
        (Vec::new(), [0.0; 4])
    }

    /// Does artboard-space point `at` land on the outline of drawable node `n` itself?
    fn hits(&self, n: u32, at: P) -> bool {
        matches!(self.def().nodes[n as usize].ty, t::SHAPE | t::TEXT_NODE | t::IMAGE | t::MODEL | t::LAYOUT)
            && self.world[n as usize].invert().is_some_and(|inv| geom::contains(&self.contours(n, 0.25), inv.apply(at)))
    }

    /// The children of `n` that are drawn, bottom first: document order, rearranged by
    /// `order`, and only the `solo` child when the node has one.
    pub(crate) fn drawn_children<'a>(&self, ab: &'a Artboard, n: u32) -> Cow<'a, [u32]> {
        let node = &ab.nodes[n as usize];
        let solo = if node.ty == t::NODE { self.u(n, p::SOLO) as usize } else { 0 };
        let shown = if solo > 0 { node.draws.get(solo - 1..solo).unwrap_or(&[]) } else { &node.draws[..] };
        if shown.iter().all(|c| self.f(*c, p::ORDER) == 0.0) {
            return Cow::Borrowed(shown);
        }
        let mut keyed: Vec<(f32, u32)> = shown.iter().map(|c| (self.f(*c, p::ORDER), *c)).collect();
        geom::sort_keyed(&mut keyed); // stable: ties keep document order
        Cow::Owned(keyed.into_iter().map(|k| k.1).collect())
    }

    /// Is node `n` drawn, or does a `solo` above it leave it out?
    fn shown(&self, n: u32) -> bool {
        let ab = self.def();
        let mut n = n;
        loop {
            let parent = ab.nodes[n as usize].parent;
            if parent == NONE {
                return true;
            }
            if !self.drawn_children(ab, parent).contains(&n) {
                return false;
            }
            n = parent;
        }
    }

    /// Does artboard-space point `at` land on node `n` or anything drawn inside it?
    fn hit_node(&self, n: u32, at: P) -> bool {
        self.hits(n, at) || self.drawn_children(self.def(), n).iter().any(|c| self.hit_node(*c, at))
    }

    /// The top-most drawable at artboard-space point `at`, in or under node `n`.
    fn top_hit(&self, n: u32, at: P) -> Option<u32> {
        self.drawn_children(self.def(), n).iter().rev().find_map(|c| self.top_hit(*c, at)).or_else(|| self.hits(n, at).then_some(n))
    }

    /// File index of the top-most drawable under surface pixel `x, y` (editor picking).
    pub fn hit_test(&self, x: f32, y: f32) -> Option<u32> {
        let at = self.view.invert()?.apply(pt(x, y));
        self.top_hit(0, at).map(|n| self.def().nodes[n as usize].obj)
    }

    /// Artboard-space bounding box `[x0, y0, x1, y1]` of an object and its descendants.
    pub fn bounds(&self, obj: u32) -> Option<[f32; 4]> {
        let n = *self.def().local.get(&obj)?;
        let mut all = Vec::new();
        let mut stack = vec![n];
        while let Some(n) = stack.pop() {
            let node = &self.def().nodes[n as usize];
            if matches!(node.ty, t::ARTBOARD | t::SHAPE | t::TEXT_NODE | t::RUN | t::IMAGE | t::MODEL | t::LAYOUT) {
                let mut cs = self.contours(n, 0.25);
                geom::transform_contours(&mut cs, &self.world[n as usize]);
                all.extend(cs);
            } else if schema::is_geometry(node.ty) {
                let mut cs = Vec::new();
                geom::flatten(&self.geometry(n), &self.world[n as usize], 0.25, &mut cs);
                all.extend(cs);
            }
            // Geometry and what is drawn inside; not what a `solo` leaves out.
            let drawn = self.drawn_children(self.def(), n);
            stack.extend(
                node.children.iter().filter(|c| schema::has_transform(self.def().nodes[**c as usize].ty) && (!node.draws.contains(c) || drawn.contains(c))),
            );
        }
        geom::bounds(&all)
    }

    /// Node -> artboard transform of an object (editor handles).
    pub fn world_matrix(&self, obj: u32) -> Option<Mat> {
        self.def().local.get(&obj).map(|n| self.world[*n as usize])
    }

    // ---- rendering -------------------------------------------------------------------

    /// The transform that fits this artboard into a `w` x `h` surface, placed at `ax`, `ay`
    /// of the room that is left over (or of the overflow): 0 = left / top, 0.5 = center,
    /// 1 = right / bottom.
    pub fn fit(&self, fit: Fit, ax: f32, ay: f32, w: f32, h: f32) -> Mat {
        let (aw, ah) = (self.width(), self.height());
        if aw <= 0.0 || ah <= 0.0 {
            return Mat::IDENTITY;
        }
        let (rx, ry) = (w / aw, h / ah);
        let (sx, sy) = match fit {
            Fit::Contain => (rx.min(ry), rx.min(ry)),
            Fit::Cover => (rx.max(ry), rx.max(ry)),
            Fit::Fill => (rx, ry),
            Fit::None => (1.0, 1.0),
            Fit::FitWidth => (rx, rx),
            Fit::FitHeight => (ry, ry),
            Fit::ScaleDown => (rx.min(ry).min(1.0), rx.min(ry).min(1.0)),
        };
        Mat { a: sx, b: 0.0, c: 0.0, d: sy, tx: (w - aw * sx) * ax, ty: (h - ah * sy) * ay }
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

/// The runtime value of a property as the file stores it; None for what the runtime does not read.
fn pval(key: u16, raw: &Value) -> Option<PVal> {
    Some(match (schema::prop(key)?.kind, raw) {
        (Kind::Float, Value::Float(v)) if v.is_finite() => PVal::F(*v),
        (Kind::Color, Value::Color(c)) => PVal::C(*c),
        (Kind::Str, Value::Bytes(b)) => match std::str::from_utf8(b).ok()? {
            s if key == p::D => PVal::P(Arc::new(parse_svg_path(s).unwrap_or_default())),
            s => PVal::S(s.into()),
        },
        (Kind::Uint | Kind::Bool | Kind::Ref | Kind::PropKey | Kind::Enum(_), Value::Uint(v)) => PVal::U(*v as u32),
        _ => return None,
    })
}

/// `v` with `decimals` digits after the point, rounded. Written by hand: the standard
/// float formatter is several kilobytes of wasm.
// ponytail: the scaled value is kept within i64, so a number beyond about 9e18 / 10^decimals
// prints as that limit. An f32 has 7 significant digits; nobody formats one that large.
fn fixed(v: f32, decimals: u32) -> String {
    let n = (v as f64 * 10i64.pow(decimals) as f64).round().clamp(-9e18, 9e18) as i64;
    let (mut rest, mut out) = (n.unsigned_abs(), Vec::new());
    // The digits from the last one back, with the point after `decimals` of them.
    while rest > 0 || out.len() <= decimals as usize {
        out.extend((decimals > 0 && out.len() == decimals as usize).then_some(b'.'));
        out.push(b'0' + (rest % 10) as u8);
        rest /= 10;
    }
    out.extend((n < 0).then_some(b'-'));
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `v` mixed over the current value `cur` of property `prop`: numbers, colors and paths
/// interpolate, the rest switches half-way.
fn mixed(cur: Option<&PVal>, v: PVal, mix: f32, prop: u16) -> PVal {
    match (cur, v) {
        (cur, PVal::F(b)) => {
            let a = if let Some(PVal::F(a)) = cur { *a } else { schema::default(prop) };
            PVal::F(a + (b - a) * mix)
        }
        (cur, PVal::C(b)) => PVal::C(lerp_color(if let Some(PVal::C(a)) = cur { *a } else { schema::DEFAULT_COLOR }, b, mix)),
        (Some(PVal::P(a)), PVal::P(b)) => PVal::P(Arc::new(lerp_path(a, &b, mix))),
        (Some(cur), _) if mix < 0.5 => cur.clone(),
        (_, v) => v,
    }
}

/// Weight of one animation of a `direct` state.
fn weight(vm: &[VmVal], &(_, value, slot): &(usize, f32, Option<usize>)) -> f32 {
    slot.map_or(value, |s| vm[s].num).clamp(0.0, 1.0)
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

/// Push the Events of the cues that playback from `from` to `to` (elapsed frames, before
/// looping is applied) passed: a cue on frame f is passed when `from <= f < to`, on every
/// lap of a loop, and in both directions of a pingpong.
// ponytail: a cue fires at most once per call, however many laps one long step covers,
// and not at all when time runs backwards (negative speed).
fn cues(a: &Anim, from: f64, to: f64, out: &mut Vec<u32>) {
    let d = a.duration as f64;
    for &(frame, event) in &a.cues {
        let frame = frame as f64;
        let passed = |at: f64, lap: f64| {
            // The first time at or after `from` that the playhead is on `at`.
            let next = if lap > 0.0 && from > at { at + ((from - at) / lap).ceil() * lap } else { at };
            from <= next && next < to
        };
        let hit = match a.looping {
            0 => passed(frame, 0.0),
            1 => passed(frame, d),
            _ => passed(frame, 2.0 * d) || passed(2.0 * d - frame, 2.0 * d),
        };
        if hit && (0.0..=d).contains(&frame) {
            out.push(event);
        }
    }
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
