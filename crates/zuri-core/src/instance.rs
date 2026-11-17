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
/// The Properties of an artboard, and the NestedArtboard of it that an instance is made for.
type Fed<'a> = (&'a [VmVal], &'a crate::file::Nest);
/// What an artboard binds `into` a nested one goes down into it: the values that differ,
/// and (with `fires`) a trigger fired this frame. True when anything went.
fn feed(vm: &[VmVal], binds: &[(usize, usize)], child: &mut Instance, fires: bool) -> bool {
    let mut any = false;
    for &(from, to) in binds {
        let (src, dst, fired) = (&vm[from], &mut child.vm[to], fires && vm[from].fired);
        if dst.num != src.num || dst.color != src.color || dst.text != src.text || fired {
            (dst.num, dst.color, dst.text, dst.fired) = (src.num, src.color, src.text.clone(), dst.fired || fired);
            child.dirty = true;
            any = true;
        }
    }
    any
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

/// How far a press may go along a Layout that scrolls before the Layout goes with it,
/// in pixels of the surface, until the host says otherwise (`set_drag_threshold`).
const SLOP: f32 = 8.0;
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
    /// Per node, for a Layout that scrolls: how far it can go (what does not fit).
    far: Vec<f32>,
    /// The Layouts a pointer is dragging, one that scrolls sideways and one that scrolls
    /// up and down: each with where the pointer took hold along what it scrolls (in the
    /// Layout's space), how far it was scrolled then, and whether the pointer has gone
    /// further than the drag threshold since.
    drag: [Option<(u32, f32, f32, bool)>; 2],
    /// In the instance the host holds: how far a press may go before it is a scroll
    /// (see `set_drag_threshold`).
    slop: f32,
    /// In the instance the host holds: the pointer whose press is open (it went down and
    /// has not been let go, nor left), by the host's number for it.
    press: Option<u32>,
    /// In the instance the host holds: the pointers that came down beside an open press
    /// and have not been let go. They are not the scene's.
    beside: Vec<u32>,
    /// In the instance the host holds: the press has moved a Layout, here or in what is
    /// nested. It is a scroll, and until it is let go the pointer is on nothing.
    swiped: bool,
    /// Per node, when the artboard has Layouts: the box of a Text's lines.
    lines: Vec<[f32; 4]>,
    vm: Vec<VmVal>,
    player: Player,
    /// (NestedArtboard node, its instances: as many copies as its `count` says), parallel
    /// to `Artboard::nests`.
    pub(crate) nested: Vec<(u32, Vec<Instance>)>,
    /// How many artboards this one is nested in.
    depth: u32,
    events: Vec<u32>,
    /// Artboard -> surface pixels, from the last render. Pointer input is mapped through it.
    pub(crate) view: Mat,
    dirty: bool,
    hover: Vec<bool>,
    pressed: Vec<bool>,
    /// Per Listener: the press it saw begin ended for it when the pointer left, so the
    /// release, if the pointer comes back for it, is not a second one.
    let_go: Vec<bool>,
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
    /// A ShaderPaint here or in what is nested, as of the last advance.
    has_shader: bool,
    /// A ShaderPaint in this artboard itself.
    shades: bool,
    /// At construction: a list here or below is shorter than its `count` says.
    short: bool,
    /// In the instance the host holds: how many more nested instances it may make, at
    /// every level together.
    room: u32,
    /// State of the generator behind `Transition::chance`. Every instance starts from the
    /// same seed, so a given sequence of calls always plays out the same way.
    rng: u32,
}

impl Instance {
    pub fn new(file: &Arc<File>, artboard: usize) -> Result<Instance> {
        let mut room = MAX_NESTED;
        let mut inst = Instance::with_depth(file, artboard, 0, &mut room, None)?;
        inst.room = room;
        if inst.short {
            bail!("more than {MAX_NESTED} nested artboard instances (does an artboard nest itself, or is a `count` that large?)");
        }
        Ok(inst)
    }

    /// `fed` is the artboard this one is nested in, and how. What that binds `into` this
    /// one is there before this one's own lists are made, which may be as long as that
    /// says; and it comes after this one has started, so that a `change` Listener here
    /// hears it at the first advance.
    fn with_depth(file: &Arc<File>, artboard: usize, depth: u32, budget: &mut u32, fed: Option<Fed>) -> Result<Instance> {
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
            far: Vec::new(),
            drag: [None; 2],
            slop: SLOP,
            press: None,
            beside: Vec::new(),
            swiped: false,
            lines: Vec::new(),
            vm: ab.vm.iter().map(|v| VmVal { num: v.num, color: v.color, text: v.text.clone(), fired: false, pending: false }).collect(),
            player: Player::None,
            nested: ab.nests.iter().map(|nest| (nest.node, Vec::new())).collect(),
            depth,
            events: Vec::new(),
            view: Mat::IDENTITY,
            dirty: true,
            hover: Vec::new(),
            pressed: Vec::new(),
            let_go: Vec::new(),
            seen: Vec::new(),
            synced: ab.binds.iter().map(|_| None).collect(),
            texts: ab.binds.iter().map(|_| None).collect(),
            time: 0.0,
            image_gen: 0,
            textures: HashMap::new(),
            requests: Vec::new(),
            has_shader: false,
            shades: ab.nodes.iter().any(|n| n.ty == t::SHADER_PAINT),
            short: false,
            room: 0,
            rng: SEED,
        };
        if let Some((vm, nest)) = fed {
            inst.start(nest.play);
            // (Not a trigger: one fired before the copy was there is not for it.)
            feed(vm, &nest.binds, &mut inst, false);
        }
        inst.count();
        inst.resize(budget);
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
        if !matches!(key, p::X | p::Y | p::ROTATION | p::SCALE_X | p::SCALE_Y | p::OPACITY | p::ORDER | p::SOLO | p::SCROLLED) {
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
        self.let_go = vec![false; seen.len()];
        self.seen = seen;
        self.dirty = true;
        true
    }

    /// Play the first state machine, or failing that the first animation that no Joystick scrubs.
    pub fn autoplay(&mut self) {
        if !self.play_state_machine(0) {
            // An animation that a Joystick scrubs is a set of poses, not something to play.
            let ab = self.def();
            if let Some(first) = (0..ab.anims.len()).find(|a| !ab.joysticks.iter().any(|joystick| joystick.2 == *a)) {
                self.play_animation(first);
            }
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
        // ponytail: a `count` that the animation keys is followed at the next advance, and
        // the copies made then start from their beginning.
        let mut room = self.room;
        for (nest, (_, copies)) in file.artboards[self.ab].nests.iter().zip(&mut self.nested) {
            for child in copies {
                child.start(nest.play);
                child.step(seconds, &mut room);
                child.take_events(); // a seek is a jump: nothing on the way is reported
            }
        }
        self.room = room;
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
        let mut room = self.room;
        let active = self.step(dt, &mut room);
        self.room = room;
        active
    }

    /// `advance`, with how many more instances the lists in here may make between them.
    fn step(&mut self, dt: f32, room: &mut u32) -> bool {
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
        for (nest, (_, copies)) in file.artboards[self.ab].nests.iter().zip(&mut self.nested) {
            for child in copies {
                feed(&self.vm, &nest.binds, child, true);
            }
        }
        for v in &mut self.vm {
            v.fired = std::mem::take(&mut v.pending);
        }
        self.apply_binds();
        // Joysticks pose last: where a handle is, from -1 to 1, is how far into its
        // animation the scene is (beyond them, its first or last key holds). An animation
        // that is itself being played is left to play, so that it can be watched. Nothing
        // was set since the last advance: nothing to pose again.
        // ponytail: a property that the playing animation and a joystick both animate is
        // written twice a frame, so such a scene never settles; pose only what the
        // player did not write if scenes like that turn up.
        let ab = &file.artboards[self.ab];
        if self.dirty && !ab.joysticks.is_empty() {
            let played = if let Player::Anim { idx, .. } = self.player { idx } else { usize::MAX };
            for &(n, handle, a) in &ab.joysticks {
                if a != played {
                    self.apply(&ab.anims[a], (self.f(n, handle) + 1.0) * 0.5 * ab.anims[a].duration, 1.0);
                }
            }
            // What a joystick posed may be bound to a Property.
            self.apply_binds();
        }
        // A `count` that a key or a Bind changed: the copies follow. Past what an instance
        // may hold in all a list stays as long as it got, until there is room.
        self.resize(room);
        // A ShaderPaint at any level keeps the root redrawing (see below), while it is there.
        self.has_shader = self.shades;
        for child in self.nested.iter_mut().flat_map(|(_, copies)| copies) {
            active |= child.step(dt, room);
            self.has_shader |= child.has_shader;
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

    /// The number a Bind gives the object from its Property.
    fn bound(&self, b: &crate::file::BindDef) -> f32 {
        let num = self.vm[b.slot].num;
        // Only a bind to the object converts: mul, add, the limits, rounding, format.
        let n = geom::within(num * b.mul + b.add, b.min, b.max);
        match b.round {
            _ if b.dir != 0 => num,
            0 => n,
            // A half goes away from 0, as it does when a number is written as text.
            1 => n.round(),
            2 => n.floor(),
            _ => n.ceil(),
        }
    }

    fn apply_binds(&mut self) {
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        for (i, b) in ab.binds.iter().enumerate() {
            let Some(def) = schema::prop(b.prop) else { continue };
            let (src, from) = (&self.vm[b.slot], &ab.vm[b.slot]);
            let number = self.bound(b);
            let v = match def.kind {
                Kind::Float => PVal::F(number),
                Kind::Color => PVal::C(src.color),
                Kind::Str if b.prop == p::D => PVal::P(Arc::new(parse_svg_path(&src.text).unwrap_or_default())),
                // The text is built when the Property changes, not every frame.
                Kind::Str => match &self.texts[i] {
                    Some((of, text)) if *of == src.value() => PVal::S(text.clone()),
                    _ => {
                        let text = match from.ty {
                            0 | 1 => fixed(number, b.decimals).into(),
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
                Kind::Uint | Kind::Bool | Kind::Enum(_) => PVal::U(number.max(0.0) as u32),
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
            // The box around every copy, from where the first stands.
            t::NESTED_ARTBOARD => {
                let (mut w, mut h, mut any) = (0f32, 0f32, false);
                for (_, copies) in self.nested.iter().filter(|nest| nest.0 == n) {
                    for (c, at) in copies.iter().zip(self.spots(n, copies)) {
                        (w, h, any) = (w.max(at.tx + c.width()), h.max(at.ty + c.height()), true);
                    }
                }
                if any {
                    vec![Contour { pts: vec![pt(0.0, 0.0), pt(w, 0.0), pt(w, h), pt(0.0, h)], closed: true }]
                } else {
                    Vec::new()
                }
            }
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
    // skinned Path is measured as it was last drawn. Lines of a wrapping Layout are packed
    // at the start across the flow; only Layouts grow or stretch, and only upright ones.
    fn lay_out(&mut self, ab: &Artboard) {
        self.sizes.clear();
        self.sizes.resize(ab.nodes.len(), (0.0, 0.0));
        self.slots = self.sizes.clone();
        self.far.clear();
        self.far.resize(ab.nodes.len(), 0.0);
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
        // The padding of a Layout before its children and on both sides together, each along and across.
        let pads = |this: &Instance, l: u32, column: bool| {
            let side = |key: u16| this.f(l, p::PADDING) + this.f(l, key);
            let lead = (side(p::PADDING_LEFT), side(p::PADDING_TOP));
            (axes(column, lead), axes(column, (lead.0 + side(p::PADDING_RIGHT), lead.1 + side(p::PADDING_BOTTOM))))
        };
        // A width or a height kept within the Layout's least and most (the most wins),
        // and a size kept so both ways.
        let fit = |this: &Instance, l: u32, v: f32, tall: bool| {
            let (least, most) = if tall { (p::MIN_HEIGHT, p::MAX_HEIGHT) } else { (p::MIN_WIDTH, p::MAX_WIDTH) };
            v.max(this.f(l, least)).min(this.f(l, most))
        };
        let bounded = |this: &Instance, l: u32, (w, h): (f32, f32)| (fit(this, l, w, false), fit(this, l, h, true));
        // The size along its flow that placing gave a Layout that wraps, where that is
        // not the size it was sized with: a parent's `grow` or `stretch` (0: none yet).
        // Its lines break there, so everything is sized once more.
        // ponytail: twice at most. A Layout that is sized by how such a child wrapped
        // and in turn decides that child's size would need a third round.
        let mut given = vec![0.0f32; ab.nodes.len()];
        for _ in 0..2 {
            let (mut items, mut again) = (Vec::new(), false);
            // Sizes, from the inside out: a Layout without a width (or height) of its own is
            // as large as what it holds.
            for &l in ab.layouts.iter().rev() {
                let (column, gap) = (self.u(l, p::FLOW) == 1, self.f(l, p::GAP));
                let pad = pads(self, l, column).1;
                let own = axes(column, (self.f(l, p::WIDTH), self.f(l, p::HEIGHT)));
                // Its lines: one, unless it wraps and has a size along the flow (its own,
                // or the one it was given), where a child that does not fit starts the
                // next. `line` is the one being filled, with a gap after each child;
                // `done` is the lines before it, across; `flat` is all of it on one line.
                // The size along the flow that the lines break at: what it was given, else
                // its own, else (fitting its content) all the room it may take.
                let wide = if given[l as usize] > 0.0 { given[l as usize] } else { fit(self, l, if own.0 > 0.0 { own.0 } else { f32::INFINITY }, column) };
                let room = if self.u(l, p::WRAP) != 0 && wide > 0.0 { wide - pad.0 } else { f32::INFINITY };
                let (mut held, mut ends, mut done, mut line, mut flat) = (Vec::new(), Vec::new(), 0.0f32, (0.0f32, 0.0f32), 0.0f32);
                for &c in &ab.nodes[l as usize].draws {
                    let Some(b) = boxed(self, c) else { continue };
                    let size = axes(column, (b[2] - b[0], b[3] - b[1]));
                    if held.len() > ends.last().copied().unwrap_or(0) && line.0 + size.0 > room {
                        ends.push(held.len());
                        (done, line) = (done + line.1 + gap, (0.0, 0.0));
                    }
                    (line, flat) = ((line.0 + size.0 + gap, line.1.max(size.1)), flat + size.0 + gap);
                    held.push((c, b));
                }
                ends.push(held.len());
                // What it holds: all of it along (a Layout that wraps is not sized by where its lines broke), its lines across.
                let hug = (flat - if held.is_empty() { 0.0 } else { gap }, done + line.1);
                let size = (if own.0 > 0.0 { own.0 } else { (hug.0 + pad.0).max(0.0) }, if own.1 > 0.0 { own.1 } else { (hug.1 + pad.1).max(0.0) });
                self.sizes[l as usize] = bounded(self, l, axes(column, size));
                items.push((held, ends, axes(column, self.sizes[l as usize]).0));
            }
            // Places, from the outside in: a Layout's size is final once its parent has placed it.
            for (&l, (held, ends, sized)) in ab.layouts.iter().zip(items.iter().rev()) {
                let (column, gap, justify, align) = (self.u(l, p::FLOW) == 1, self.f(l, p::GAP), self.u(l, p::JUSTIFY), self.u(l, p::ALIGN_ITEMS));
                let (lead, pad) = pads(self, l, column);
                let (room, wraps) = (axes(column, self.sizes[l as usize]), self.u(l, p::WRAP) != 0);
                if wraps && room.0 != *sized {
                    (given[l as usize], again) = (room.0, true);
                }
                let room = (room.0 - pad.0, room.1 - pad.1);
                // How much larger a child Layout is in its parent than in itself, along and
                // across the flow: None for one that is not a Layout, or is turned (it keeps its size).
                let scale = |this: &Instance, c: u32| {
                    let by = axes(column, (this.f(c, p::SCALE_X).abs(), this.f(c, p::SCALE_Y).abs()));
                    (ab.nodes[c as usize].ty == t::LAYOUT && this.f(c, p::ROTATION) == 0.0 && by.0 > 0.0 && by.1 > 0.0).then_some(by)
                };
                // One that scrolls starts its content at the start however it is justified,
                // and `over` is how much longer its line is than its room.
                let (scrolls, mut over) = (self.u(l, p::SCROLL) != 0, 0.0f32);
                let (mut from, mut top) = (0, lead.1);
                for &end in ends {
                    let line = &held[from..end];
                    from = end;
                    // The room to spare along the flow, the `grow` of the Layouts that share
                    // it, and the room across: all of it, or in a Layout that wraps what
                    // the largest child on this line takes.
                    let (mut spare, mut shares, mut tall) = (room.0 - gap * line.len().saturating_sub(1) as f32, 0.0, 0.0f32);
                    for &(c, b) in line {
                        let size = axes(column, (b[2] - b[0], b[3] - b[1]));
                        (spare, tall) = (spare - size.0, tall.max(size.1));
                        if scale(self, c).is_some() {
                            shares += self.f(c, p::GROW).max(0.0);
                        }
                    }
                    if !wraps {
                        tall = room.1;
                    }
                    over = -spare;
                    if scrolls {
                        spare = spare.max(0.0);
                    }
                    let mut share = 0.0;
                    if shares > 0.0 {
                        (share, spare) = (spare.max(0.0) / shares, spare.min(0.0));
                    }
                    let between = if justify == 3 && line.len() > 1 { gap + spare.max(0.0) / (line.len() - 1) as f32 } else { gap };
                    let mut at = lead.0 + [0.0, spare * 0.5, spare, 0.0][justify.min(3) as usize];
                    for &(c, mut b) in line {
                        if let Some(by) = scale(self, c) {
                            // Its share of the room to spare, and with `stretch` all of the room across.
                            // ponytail: what its least or most takes back from that share is
                            // not handed to the other Layouts that grow, nor seen by `justify`:
                            // the line just ends short. Flexbox shares it out again in rounds.
                            let mut size = axes(column, self.sizes[c as usize]);
                            size.0 += share * self.f(c, p::GROW).max(0.0) / by.0;
                            if align == 3 {
                                size.1 = (tall / by.1).max(0.0);
                            }
                            self.sizes[c as usize] = bounded(self, c, axes(column, size));
                            b = boxed(self, c).unwrap_or(b);
                        }
                        let (low, size) = (axes(column, (b[0], b[1])), axes(column, (b[2] - b[0], b[3] - b[1])));
                        let across = top + [0.0, (tall - size.1) * 0.5, tall - size.1, 0.0][align.min(3) as usize];
                        self.slots[c as usize] = axes(column, (at - low.0, across - low.1));
                        at += size.0 + between;
                    }
                    top += tall + gap;
                }
                if scrolls {
                    // What does not fit, along the flow or (lines, in one that wraps) across
                    // it, is how far it can go; its children move back by how far it has gone.
                    // (Under half a unit is what rounding leaves, not something to scroll.)
                    let far = if wraps { top - gap - lead.1 - room.1 } else { over };
                    let far = if far > 0.5 { far } else { 0.0 };
                    let gone = geom::within(self.f(l, p::SCROLLED), 0.0, far);
                    // The property is kept to where the content is, so that what reads it
                    // (a Bind both ways, for a host that adds to it) never has to unwind it.
                    self.set(l, p::SCROLLED, PVal::F(gone));
                    self.far[l as usize] = far;
                    for &(c, _) in held {
                        let slot = &mut self.slots[c as usize];
                        if column != wraps {
                            slot.1 -= gone;
                        } else {
                            slot.0 -= gone;
                        }
                    }
                }
            }
            if !again {
                break;
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
        let short = |degrees: f32| geom::wrap(degrees + 180.0, 360.0) - 180.0;
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
        // A copy takes one axis or both. What is copied is the target's value times
        // `factor`, and with `offset` the object's own value on top (its x / y and rotation
        // added, its size multiplied); the result stays within `min`..`max` (`max` wins if
        // they cross).
        // ponytail: everything is in artboard space, and the own value is the object's
        // property: under a turned or scaled parent an offset is not along the parent's
        // axes, and what a Layout or a parent Bone adds to its place is not in it. Both
        // are the `space` property that is still to come.
        let (axis, limit) = (self.u(c, p::AXIS), |v: f32| v.max(self.f(c, p::MIN)).min(self.f(c, p::MAX)));
        let take = |other: u32, target: f32, own: f32| if axis == other { own } else { target };
        let factor = || self.f(c, p::FACTOR);
        let node = self.def().nodes[c as usize].parent;
        let own = |key: u16, none: f32| if self.u(c, p::OFFSET) != 0 { self.f(node, key) } else { none };
        match self.u(c, p::CONSTRAIN) {
            0 => place = Some((take(2, limit(tw.tx * factor() + own(p::X, 0.0)), w.tx), take(1, limit(tw.ty * factor() + own(p::Y, 0.0)), w.ty))),
            1 => {
                // The target's turn as a number that goes on past 180, so that a rate and
                // the limits see it turn steadily: what its matrix reads, by the whole
                // turns that bring it nearest to its own `rotation`.
                // ponytail: an ancestor that spins, or a mirrored one, still makes it jump;
                // that needs the turns of the whole chain counted.
                let turns = ((self.f(target, p::ROTATION) - trot) / 360.0 + 0.5).floor();
                turn = Some(limit((trot + turns * 360.0) * factor() + own(p::ROTATION, 0.0)));
            }
            2 => {
                // With `offset` the object's own size is multiplied in, and a mirrored
                // object stays mirrored (a mirror is on x here: see `parts`). The limits
                // are on the size, whichever way it faces.
                let mirror = if own(p::SCALE_X, 1.0) * own(p::SCALE_Y, 1.0) < 0.0 { -1.0 } else { 1.0 };
                let limited = |size: f32| limit(size.abs()).copysign(size);
                let (tsx, tsy) = (tsx * factor() * own(p::SCALE_X, 1.0).abs() * mirror, tsy * factor() * own(p::SCALE_Y, 1.0).abs());
                scale = Some((take(2, limited(tsx), sx), take(1, limited(tsy), sy)));
            }
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
            let (sin, cos) = geom::sin_cos(((geom::wrap(to - rot + 180.0, 360.0) - 180.0) * strength).to_radians());
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
    /// `card/title` is the property `title` of the artboard nested as `card`, and so on
    /// down: the nested instance a path begins with, and the rest of the path.
    // (On bytes: slicing a str brings its panic messages into the wasm.)
    fn child<'a>(&self, path: &'a str) -> Option<(usize, usize, &'a str)> {
        let bytes = path.as_bytes();
        let cut = bytes.iter().position(|b| *b == b'/')?;
        let (ab, mut rest) = (self.def(), path.get(cut + 1..)?);
        let named = |node: u32| self.file.objects[ab.nodes[node as usize].obj as usize].bytes(p::NAME) == Some(&bytes[..cut]);
        let i = self.nested.iter().position(|(node, _)| named(*node))?;
        // In a list a number comes next, which copy from 0: "rows/2/title". Without one it
        // is the first.
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let mut copy = 0;
        if digits > 0 && rest.as_bytes().get(digits) == Some(&b'/') {
            // (More digits than a list can be long is past its end, and cannot overflow.)
            copy = rest.bytes().take(digits).fold(0, |n: usize, b| (n * 10 + (b - b'0') as usize).min(MAX_NESTED as usize));
            rest = rest.get(digits + 1..)?;
        }
        (copy < self.nested[i].1.len()).then_some((i, copy, rest))
    }
    /// The instance a path ends in, and the property's own name there.
    fn at<'a>(&self, path: &'a str) -> (&Self, &'a str) {
        let Some((i, copy, rest)) = self.child(path) else { return (self, path) };
        self.nested[i].1[copy].at(rest)
    }
    fn at_mut<'a>(&mut self, path: &'a str) -> (&mut Self, &'a str) {
        let Some((i, copy, rest)) = self.child(path) else { return (self, path) };
        self.nested[i].1[copy].at_mut(rest)
    }
    /// A Bind of a `count` from a Property is applied as soon as the Property is written,
    /// and before the first advance, so that a list is as long as the host says without
    /// waiting for one.
    fn count(&mut self) {
        let file = self.file.clone();
        for b in file.artboards[self.ab].binds.iter().filter(|b| b.prop == p::COUNT && b.dir == 0) {
            self.set(b.node, p::COUNT, PVal::F(self.bound(b)));
        }
    }
    /// Make and drop copies of each NestedArtboard until there are as many as its `count`
    /// says (a part of one is none, and less than none too), or `budget` is spent: how
    /// many more instances may be made, here and below. Every instance takes one of it,
    /// so nothing is ever made in part: a list that is short of room stays as long as it
    /// got, and grows when there is room.
    // ponytail: nesting is capped at 8 levels, which also stops an artboard that nests
    // itself; the budget caps the total, because 8 levels of 8 children would be 16
    // million instances. The lists take the room in the order they are come to: what a
    // later one gives up, an earlier one takes the next time (every advance).
    fn resize(&mut self, budget: &mut u32) {
        let file = self.file.clone();
        self.short = false;
        for (i, nest) in file.artboards[self.ab].nests.iter().enumerate() {
            let want = if self.depth < 8 { self.f(nest.node, p::COUNT) as usize } else { 0 };
            let have = self.nested[i].1.len();
            for gone in self.nested[i].1.drain(want.min(have)..) {
                *budget += 1 + gone.census();
            }
            for _ in have..want {
                if *budget == 0 {
                    self.short = true;
                    break;
                }
                *budget -= 1;
                let Ok(child) = Instance::with_depth(&file, nest.artboard, self.depth + 1, budget, Some((&self.vm, nest))) else { break };
                self.short |= child.short;
                self.nested[i].1.push(child);
            }
            self.dirty |= self.nested[i].1.len() != have;
        }
    }
    /// After the host wrote a number here: the lists whose `count` is bound to it are as
    /// long as it says at once, and so are the lists further down that it is bound `into`.
    fn settle(&mut self, room: &mut u32) {
        self.count();
        self.resize(room);
        let file = self.file.clone();
        for (nest, (_, copies)) in file.artboards[self.ab].nests.iter().zip(&mut self.nested) {
            for child in copies.iter_mut().filter(|_| !nest.binds.is_empty()) {
                if feed(&self.vm, &nest.binds, child, false) {
                    child.settle(room);
                }
            }
        }
    }
    /// How many instances are nested in this one, at every level.
    fn census(&self) -> u32 {
        self.nested.iter().flat_map(|(_, copies)| copies).map(|c| 1 + c.census()).sum()
    }
    /// Where each copy of a NestedArtboard stands from the first: one after the other
    /// along its `flow`, each as long as it is, with its `gap` between them.
    pub(crate) fn spots<'a>(&self, node: u32, copies: &'a [Instance]) -> impl Iterator<Item = Mat> + 'a {
        let (column, gap, mut at) = (self.u(node, p::FLOW) == 1, self.f(node, p::GAP), 0.0);
        copies.iter().map(move |c| {
            let (tx, ty) = if column { (0.0, at) } else { (at, 0.0) };
            at += gap + if column { c.height() } else { c.width() };
            Mat { tx, ty, ..Mat::IDENTITY }
        })
    }
    /// The type (0 number, 1 bool, 2 trigger, 3 color, 4 string, 5 enum) of the property a
    /// name or a path refers to, if there is one.
    pub fn property_type(&self, name: &str) -> Option<u32> {
        let (this, name) = self.at(name);
        this.def().vm.iter().find(|v| v.name.as_bytes() == name.as_bytes()).map(|v| v.ty)
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
    pub fn set_number(&mut self, path: &str, v: f32) -> bool {
        let (this, name) = self.at_mut(path);
        let Some(s) = this.slot(name, &[0, 1, 5]).filter(|_| v.is_finite()) else { return false };
        let options = this.def().vm[s].options.len();
        if this.def().vm[s].ty == 5 && !(v.fract() == 0.0 && v >= 0.0 && (v as usize) < options) {
            return false;
        }
        this.dirty |= this.vm[s].num != v;
        this.vm[s].num = v;
        // A list whose `count` is bound to this is as long as it says at once, so that
        // the host can write to the new copies.
        if !this.nested.is_empty() {
            let mut room = self.room;
            self.at_mut(path).0.settle(&mut room);
            self.room = room;
        }
        true
    }
    pub fn set_bool(&mut self, name: &str, v: bool) -> bool {
        self.set_number(name, v as u8 as f32)
    }
    pub fn number(&self, name: &str) -> Option<f32> {
        let (this, name) = self.at(name);
        this.slot(name, &[0, 1, 5]).map(|s| this.vm[s].num)
    }
    pub fn set_color(&mut self, name: &str, argb: u32) -> bool {
        let (this, name) = self.at_mut(name);
        let Some(s) = this.slot(name, &[3]) else { return false };
        this.dirty |= this.vm[s].color != argb;
        this.vm[s].color = argb;
        true
    }
    pub fn color(&self, name: &str) -> Option<u32> {
        let (this, name) = self.at(name);
        this.slot(name, &[3]).map(|s| this.vm[s].color)
    }
    /// Set a string Property, or an enum Property to the Option of that name.
    pub fn set_string(&mut self, path: &str, v: &str) -> bool {
        let (this, name) = self.at_mut(path);
        if let Some(s) = this.slot(name, &[5]) {
            // (From the top: the room for what a list may then make is kept there.)
            let option = this.def().vm[s].options.iter().position(|o| &**o == v);
            return option.is_some_and(|i| self.set_number(path, i as f32));
        }
        let Some(s) = this.slot(name, &[4]) else { return false };
        this.dirty |= &*this.vm[s].text != v;
        this.vm[s].text = v.into();
        true
    }
    /// A string Property's text, or the name of the Option an enum Property is on.
    pub fn string(&self, name: &str) -> Option<&str> {
        let (this, name) = self.at(name);
        let s = this.slot(name, &[4, 5])?;
        let def = &this.def().vm[s];
        Some(if def.ty == 5 { def.options.get(this.vm[s].num as usize).map_or("", |o| o) } else { &this.vm[s].text })
    }
    pub fn fire(&mut self, name: &str) -> bool {
        let (this, name) = self.at_mut(name);
        let Some(s) = this.slot(name, &[2]) else { return false };
        this.vm[s].fired = true;
        this.dirty = true;
        true
    }

    /// Names of the events `take_events` can report: this artboard's, then those of the
    /// artboards nested in it (their events bubble up), whether or not a list of them has
    /// any copies just now.
    pub fn event_names(&self) -> Vec<&str> {
        let (mut names, mut seen, mut todo) = (Vec::new(), Vec::new(), vec![self.ab]);
        while let Some(ab) = todo.pop() {
            if !seen.contains(&ab) {
                seen.push(ab);
                let def = &self.file.artboards[ab];
                for name in def.events.iter().map(String::as_str) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                todo.extend(def.nests.iter().rev().map(|nest| nest.artboard));
            }
        }
        names
    }

    /// Names of the events emitted since the last call.
    pub fn take_events(&mut self) -> Vec<String> {
        let file = self.file.clone();
        self.take_reports().into_iter().filter_map(|e| file.objects.get(e as usize)?.str(p::NAME).map(String::from)).collect()
    }

    /// The Events (as objects of the file) emitted since the last call: what
    /// `take_events` names. A host that plays sounds asks `event_sound` about each.
    pub fn take_reports(&mut self) -> Vec<u32> {
        let mut events = std::mem::take(&mut self.events);
        // Events from nested artboards bubble up to the host.
        for child in self.nested.iter_mut().flat_map(|(_, copies)| copies) {
            events.extend(child.take_reports());
        }
        events
    }

    // ---- keys and pointer ------------------------------------------------------------

    /// Report a key going down or up, named as the web's `KeyboardEvent.key` names it. True
    /// when a Listener reacted (a host uses that to keep the key from doing anything else).
    pub fn key(&mut self, key: &str, down: bool) -> bool {
        // Nested artboards hear it too, unless a `solo` hides them.
        let mut nested = std::mem::take(&mut self.nested);
        let mut heard = false;
        for (node, copies) in &mut nested {
            let shown = self.shown(*node);
            for child in copies {
                heard |= shown && child.key(key, down);
            }
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

    /// How far, in pixels of the surface, a press may go along a Layout that scrolls
    /// before it is a scroll and no longer a tap: 8 until set, and the host's to say (it
    /// knows the pointer and the screen). Less than nothing, without end, or not a number
    /// at all is not taken.
    pub fn set_drag_threshold(&mut self, pixels: f32) {
        if (0.0..f32::INFINITY).contains(&pixels) {
            self.slop = pixels;
        }
    }

    /// Feed a pointer event in surface pixels (the space of the last `render`), for a
    /// host that has one pointer with one button: `pointer_of` with both at 0.
    pub fn pointer(&mut self, kind: Pointer, x: f32, y: f32) {
        self.pointer_of(0, 0, kind, x, y);
    }

    /// Feed a pointer event in surface pixels (the space of the last `render`) from a
    /// host with several pointers (fingers, a mouse, a pen) and buttons: `pointer` is the
    /// host's number for the pointer, `button` the button that went down or up (0 is the
    /// primary one; a finger has only that). True when the event was the scene's.
    ///
    /// The scene has one pointer and one button. A press begins when a primary button
    /// goes down and ends when it goes up or its pointer leaves; what another button
    /// does is not the scene's. While a press is open no other pointer is the scene's,
    /// and one that came down beside it is not until it has been let go or has left, or
    /// presses again once the first is over.
    ///
    /// A press that moves a Layout that scrolls is a scroll: from then until the host
    /// reports `Up` or `Exit`, the Listeners hear nothing but that the pointer has left.
    /// The host has to report one of the two for every `Down`: until it does, the press
    /// is open, and the Layouts it holds follow every `Move`.
    pub fn pointer_of(&mut self, pointer: u32, button: u32, kind: Pointer, x: f32, y: f32) -> bool {
        if button != 0 && matches!(kind, Pointer::Down | Pointer::Up) {
            return false;
        }
        let other = match self.press {
            Some(pressing) => pressing != pointer,
            None => kind != Pointer::Down && self.beside.contains(&pointer),
        };
        if kind != Pointer::Move {
            self.beside.retain(|p| *p != pointer);
        }
        if other {
            // ponytail: sixteen pointers beside a press are remembered; one more is taken
            // for a new pointer when the press is over (its moves and release are heard).
            if kind == Pointer::Down && self.beside.len() < 16 {
                self.beside.push(pointer);
            }
            return false;
        }
        self.press = match kind {
            Pointer::Down => Some(pointer),
            Pointer::Move => self.press,
            _ => None,
        };
        // First the Layouts that scroll, here and in what is nested, go with the pointer
        // (a pointer that is not pressing holds none: nothing to walk the tree for).
        let moved = (self.press.is_some() || kind != Pointer::Move) && self.drags(kind, x, y, self.slop);
        // A press that has moved one is a scroll and no one's press any more: until it
        // is let go, for every Listener at every level, the pointer has left (which they
        // are told once). Let go, it is where it is again, on whatever is under it.
        let told = self.swiped;
        self.swiped = moved || (told && kind != Pointer::Down);
        // (`edge`: a press begins or is let go. A pointer that leaves the surface may
        // come back with the press still open.)
        let edge = matches!(kind, Pointer::Down | Pointer::Up);
        match (self.swiped, told, kind) {
            (false, ..) => self.hear(kind, x, y, edge),
            (true, false, _) | (true, true, Pointer::Exit) => self.hear(Pointer::Exit, x, y, edge),
            (true, true, Pointer::Up) => self.hear(Pointer::Move, x, y, edge),
            _ => {} // the scroll goes on
        }
        self.swiped &= self.press.is_some();
        true
    }

    /// Where a point of the surface is in this artboard. Seen through a view that has
    /// no way back (scaled to nothing) the pointer is nowhere: it has left.
    fn seen(&self, kind: Pointer, x: f32, y: f32) -> (Pointer, P) {
        match self.view.invert() {
            Some(inv) => (kind, inv.apply(pt(x, y))),
            None => (Pointer::Exit, pt(0.0, 0.0)),
        }
    }

    /// Every artboard nested here with the pointer event that is for it, each seeing the
    /// surface through its own view. With `there`, one that is not there to touch has
    /// the pointer as gone: one that a `solo` hides, or that is scrolled out of the
    /// Layout it is in.
    fn each_nested(&mut self, kind: Pointer, at: P, there: bool, f: &mut dyn FnMut(&mut Instance, Pointer)) {
        let mut nested = std::mem::take(&mut self.nested);
        for (node, copies) in &mut nested {
            let kind = if there && !(self.shown(*node) && self.in_view(*node, at)) { Pointer::Exit } else { kind };
            // ponytail: every copy takes the event (most only to find that it is not for
            // them); pick the one under the pointer by its box when lists get long, minding
            // that a copy that does not clip can be touched outside its box.
            let (through, spots) = (self.view.mul(&self.world[*node as usize]), self.spots(*node, copies).collect::<Vec<_>>());
            for (child, at) in copies.iter_mut().zip(spots) {
                // Where a copy is on the surface, as it is drawn: also before it has been.
                child.view = through.mul(&at);
                f(child, kind);
                self.dirty |= child.dirty;
            }
        }
        self.nested = nested;
    }

    /// The Layouts that scroll, here and in what is nested, take hold, follow and let
    /// go. True when one of them moved.
    fn drags(&mut self, kind: Pointer, x: f32, y: f32, slop: f32) -> bool {
        let (kind, at) = self.seen(kind, x, y);
        let mut moved = self.scroll(kind, at, slop);
        // Only a press has to find its Layout there: one that is held is followed
        // wherever the pointer goes, out of the box of the list around it too.
        self.each_nested(kind, at, kind == Pointer::Down, &mut |child, kind| moved |= child.drags(kind, x, y, slop));
        moved
    }

    /// The Listeners, here and in what is nested, hear a pointer event. `edge`: with it
    /// a press began or was let go, also for those that are told something else (that
    /// the pointer has left them, or that it is back): what they were told in the press
    /// before does not last into the next.
    fn hear(&mut self, kind: Pointer, x: f32, y: f32, edge: bool) {
        let (kind, at) = self.seen(kind, x, y);
        self.each_nested(kind, at, true, &mut |child, kind| child.hear(kind, x, y, edge));
        let Player::Machine { idx, .. } = self.player else { return };
        let file = self.file.clone();
        let ab = &file.artboards[self.ab];
        for (i, l) in ab.machines[idx].listeners.iter().enumerate() {
            let hit = kind != Pointer::Exit
                && match l.target {
                    Some(n) => self.shown(n) && self.hit_node(n, at) && self.in_view(n, at),
                    None => at.x >= 0.0 && at.y >= 0.0 && at.x <= self.width() && at.y <= self.height(),
                };
            let fire = match (l.event, kind) {
                (0, _) => hit && !self.hover[i],
                (1, _) => !hit && self.hover[i],
                (2, Pointer::Down) | (5, Pointer::Move) => hit,
                // A release must always be heard, or a press would stick: an `up` listener
                // without a target fires wherever the pointer is released, and any `up`
                // listener when the pointer leaves in the middle of a press that it saw
                // begin (it left the surface, or the press became a scroll). That was the
                // release: if the pointer comes back and is let go, it is not told again.
                (3, Pointer::Up) => (hit || l.target.is_none()) && !self.let_go[i],
                (3, Pointer::Exit) => self.pressed[i],
                (4, Pointer::Up) => hit && self.pressed[i],
                _ => false,
            };
            self.hover[i] = hit;
            match kind {
                Pointer::Down | Pointer::Up => (self.pressed[i], self.let_go[i]) = (hit && kind == Pointer::Down, false),
                Pointer::Exit => (self.pressed[i], self.let_go[i]) = (false, self.let_go[i] || self.pressed[i]),
                _ => {}
            }
            self.let_go[i] &= !edge;
            if fire {
                self.react(l);
            }
        }
    }

    /// A press on a Layout that scrolls takes hold of it, and once the pointer has gone
    /// further than the drag threshold along it, its content goes with the pointer, as far as it
    /// goes. A list in a list: the innermost under the pointer that scrolls sideways and
    /// the innermost that scrolls up and down are both held, each to move along its own
    /// way. True when a Layout moved.
    // ponytail: the content stops when the pointer is let go (no momentum, no spring at
    // the ends), and a nested artboard's own scrolling Layout moves as well as one
    // around it that scrolls the same way: the innermost should have the drag first.
    // A Layout whose `scrolled` an animation keys counts as moved at every step though
    // the key puts it back: a press that drifts along it is lost to what is in it.
    fn scroll(&mut self, kind: Pointer, at: P, slop: f32) -> bool {
        // Whether it scrolls up and down, and where the pointer is along that in its own space.
        let along = |this: &Instance, l: u32| {
            let (own, tall) = (this.world[l as usize].invert()?.apply(at), (this.u(l, p::FLOW) == 1) != (this.u(l, p::WRAP) != 0));
            Some((tall, if tall { own.y } else { own.x }))
        };
        let mut moved = false;
        match kind {
            Pointer::Down => {
                let mut held = [None; 2];
                for &l in self.def().layouts.iter().rev() {
                    // (One whose `scrolled` a Property is bound to is where the Property says.)
                    let bound = self.def().binds.iter().any(|b| b.node == l && b.prop == p::SCROLLED && b.dir == 0);
                    if !bound && self.far.get(l as usize).is_some_and(|far| *far > 0.0) && self.shown(l) && self.hits(l, at) && self.in_view(l, at) {
                        if let Some((tall, grip)) = along(self, l) {
                            held[tall as usize].get_or_insert((l, grip, self.f(l, p::SCROLLED), false));
                        }
                    }
                }
                self.drag = held;
            }
            Pointer::Move => {
                for i in 0..2 {
                    let Some((l, mut grip, from, gone)) = self.drag[i] else { continue };
                    let Some((tall, now)) = along(self, l) else { continue };
                    if !gone {
                        // How far the pointer has gone along it, on the surface: once that
                        // is more than the slop, the content follows from there on (the
                        // hold moves by the slop, so that nothing jumps). Against an end,
                        // where it cannot follow, the slop is not spent.
                        let m = self.view.mul(&self.world[l as usize]);
                        let (mx, my) = if tall { (m.c, m.d) } else { (m.a, m.b) };
                        let slop = slop / (mx * mx + my * my).sqrt();
                        let held = grip - slop.copysign(grip - now);
                        if (grip - now).abs() <= slop || geom::within(from + held - now, 0.0, self.far[l as usize]) == self.f(l, p::SCROLLED) {
                            continue;
                        }
                        grip = held;
                        self.drag[i] = Some((l, grip, from, true));
                    }
                    let to = geom::within(from + grip - now, 0.0, self.far[l as usize]);
                    moved |= to != self.f(l, p::SCROLLED);
                    self.set(l, p::SCROLLED, PVal::F(to));
                }
            }
            _ => self.drag = [None; 2],
        }
        moved
    }

    /// Whether a Layout that scrolls hides what is in it at `at`: outside its box.
    fn cut(&self, n: u32, at: P) -> bool {
        self.def().nodes[n as usize].ty == t::LAYOUT && self.u(n, p::SCROLL) != 0 && !self.hits(n, at)
    }

    /// Whether the Layouts that scroll around `n` show it at `at`: what is scrolled out
    /// of one is not there to touch.
    fn in_view(&self, n: u32, at: P) -> bool {
        let ab = self.def();
        let mut n = ab.nodes[n as usize].parent;
        while n != NONE {
            if self.cut(n, at) {
                return false;
            }
            n = ab.nodes[n as usize].parent;
        }
        true
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
        let start = if round { geom::wrap(position, 1.0) } else { position } * length;
        let along = |d: f32| geom::point_at(&outline, if round { geom::wrap(start + d, length) } else { start + d }).filter(|_| start + d >= 0.0 || round);
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
        self.hits(n, at) || (!self.cut(n, at) && self.drawn_children(self.def(), n).iter().any(|c| self.hit_node(*c, at)))
    }

    /// The top-most drawable at artboard-space point `at`, in or under node `n`.
    fn top_hit(&self, n: u32, at: P) -> Option<u32> {
        if self.cut(n, at) {
            return None;
        }
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
        // The remainder by hand, as `geom::wrap` has it: `%` on floats is the math
        // library's exact one, with a 128-bit division behind it (2 KB of the wasm).
        1 => (frames - (frames / d).floor() * d, false),
        _ => {
            let m = frames - (frames / (2.0 * d)).floor() * 2.0 * d;
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

/// Elastic easing: the value swings past the next key's and back, each swing smaller,
/// and is there exactly at the end. `reach` is how far a swing goes, `turn` how fast it
/// turns and `shift` what makes it start at 0 (see where a Key is built); `way` is where
/// it swings: on arriving (0), on leaving (1), or both (2).
// ponytail: the swings die away as (1 - t)^6, which is close to the usual 2^(-10 t)
// and needs no exp in the wasm; use that if a curve has to match another tool's.
fn elastic_ease([reach, turn, shift, _]: [f32; 4], way: u32, t: f32) -> f32 {
    // Arriving is the curve itself; leaving is it turned around; both is half of each.
    let (t, from, by) = match way {
        0 => (t, 0.0, 1.0),
        1 => (1.0 - t, 1.0, -1.0),
        _ if t < 0.5 => (1.0 - 2.0 * t, 0.5, -0.5),
        _ => (2.0 * t - 1.0, 0.5, 0.5),
    };
    from + by * (1.0 + reach * (1.0 - t).powi(6) * geom::sin_cos((t - shift) * turn).0)
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
    // A frame that is not a number (a handle that overflowed, say) has the first key too.
    if frame <= first.frame || frame.is_nan() {
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
        0 => t,
        way => elastic_ease(k0.ease, way - 3, t),
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
    fn elastic_easing_swings_and_arrives() {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // What a Key is built with, for an amplitude and a period (see file.rs).
        let built = |amplitude: f32, period: f32, way: u32| {
            let turn = std::f32::consts::TAU * if way == 2 { 0.5 } else { 1.0 } / period;
            [amplitude, turn, (1.0 / amplitude).asin() / turn, 0.0]
        };
        for way in 0..3 {
            for (amplitude, period) in [(1.0, 0.5), (2.5, 0.3), (1e6, 0.05)] {
                let ease = |t| elastic_ease(built(amplitude, period, way), way, t);
                assert!(near(ease(0.0), 0.0) && near(ease(1.0), 1.0), "way {way}, amplitude {amplitude}, period {period}: {} to {}", ease(0.0), ease(1.0));
            }
        }
        // Arriving: half a swing in (period 0.5: a quarter of the way) it is furthest past
        // the end, by (1 - 0.25)^6; a full period later the swing is back above it, smaller.
        let out = |t| elastic_ease(built(1.0, 0.5, 0), 0, t);
        assert!(near(out(0.25), 1.0 + 0.75f32.powi(6)), "{}", out(0.25));
        assert!(out(0.5) < 1.0 && out(0.75) > 1.0 && out(0.75) - 1.0 < 0.01);
        // More amplitude goes further (and gets there sooner: 2 x (5/6)^6 past the end, a
        // sixth of the way); leaving is arriving turned around.
        assert!(near(elastic_ease(built(2.0, 0.5, 0), 0, 1.0 / 6.0), 1.0 + 2.0 * (5.0f32 / 6.0).powi(6)));
        assert!(near(elastic_ease(built(1.0, 0.5, 1), 1, 0.3), 1.0 - out(0.7)));
        // Both: half the distance each way, and a swing still takes `period` of the whole
        // segment. The leaving half is furthest under the start a quarter of the way in
        // (half way through its own half), by 0.5^6 of half the distance.
        let both = |t| elastic_ease(built(1.0, 0.5, 2), 2, t);
        assert!(near(both(0.5), 0.5) && near(both(0.25), -0.5f32.powi(7)) && near(both(0.75), 1.0 + 0.5f32.powi(7)), "{} {}", both(0.25), both(0.75));
    }

    #[test]
    fn color_lerp_is_per_channel() {
        assert_eq!(lerp_color(0xff000000, 0xffffffff, 0.5), 0xff808080);
        assert_eq!(lerp_color(0x00102030, 0xff102030, 1.0), 0xff102030);
    }
}
