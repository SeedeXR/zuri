//! A loaded `.zur` file: the raw object tree plus everything that can be compiled once and
//! shared by all instances (scene topology, animations, state machines, view-model layout).

use crate::format::{self, RawObject};
use crate::geom::{self, parse_svg_path, sort_keyed, Path};
use crate::model::Mesh;
use crate::schema::{self, p, t, Kind};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

pub const NONE: u32 = u32::MAX;

/// A runtime property value.
#[derive(Clone, Debug, PartialEq)]
pub enum PVal {
    F(f32),
    C(u32),
    U(u32),
    S(Arc<str>),
    P(Arc<Path>),
}

/// Decoded pixels of an image, premultiplied RGBA8.
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

pub struct NodeDef {
    pub ty: u16,
    /// File index of the object.
    pub obj: u32,
    /// Node index of the parent, or `NONE` for the artboard itself.
    pub parent: u32,
    pub children: Vec<u32>,
    /// The children that are drawn in their own right, in document order.
    pub draws: Vec<u32>,
}

pub struct Key {
    pub frame: f32,
    pub val: PVal,
    pub interp: u32,
    /// What its easing needs: a cubic's control points (x1, y1, x2, y2), or for an
    /// elastic key how far it swings, how fast it turns and how far it is shifted.
    pub ease: [f32; 4],
}
pub struct Track {
    pub node: u32,
    pub prop: u16,
    pub keys: Vec<Key>,
}
pub struct Anim {
    pub name: String,
    pub fps: f32,
    pub duration: f32,
    pub looping: u32,
    pub speed: f32,
    pub tracks: Vec<Track>,
    /// (frame, Event file index): reported when playback passes the frame.
    pub cues: Vec<(f32, u32)>,
}

pub struct Cond {
    pub slot: usize,
    pub op: u32,
    pub value: f32,
    /// Compare against this slot's number instead of `value`.
    pub with: Option<usize>,
}
/// A change to a view-model Property: `action` indexes the schema's enum (set, toggle, fire, add).
pub struct Act {
    pub slot: usize,
    pub action: u32,
    pub value: f32,
}
pub struct Transition {
    pub to: usize,
    pub blend: f32,
    pub exit_time: f32,
    pub conds: Vec<Cond>,
    pub emit: Option<u32>,
    /// How the cross-fade progresses: `interp` (linear, hold, cubic) and the cubic's control points.
    pub interp: u32,
    pub ease: [f32; 4],
    pub pause: bool,
    pub chance: f32,
    pub acts: Vec<Act>,
}
pub struct State {
    pub obj: u32,
    pub kind: u32,
    pub anim: Option<usize>,
    pub speed: f32,
    pub input: Option<usize>,
    /// The BlendAnimations: (animation, `value`, slot of `input`). A `blend` state has them
    /// sorted by `value` (the position on its input's axis); a `direct` state keeps
    /// document order, and `value` or the slot's number is the weight.
    pub blend: Vec<(usize, f32, Option<usize>)>,
    pub transitions: Vec<Transition>,
}
pub struct Layer {
    pub states: Vec<State>,
    pub entry: usize,
    pub any: Vec<usize>,
}
/// Positions in the schema's `event` enum of the Listener events that are not pointer events.
pub const EVENT_CHANGE: u32 = 6;
pub const EVENT_KEY_DOWN: u32 = 7;
pub const EVENT_KEY_UP: u32 = 8;

pub struct Listener {
    pub target: Option<u32>,
    /// A `change` listener: the slot it watches.
    pub watch: Option<usize>,
    /// A `keyDown` / `keyUp` listener: the key it reacts to (None = any key).
    pub key: Option<Arc<str>>,
    pub event: u32,
    pub acts: Vec<Act>,
    pub emit: Option<u32>,
}
pub struct Machine {
    pub name: String,
    pub layers: Vec<Layer>,
    pub listeners: Vec<Listener>,
}

pub struct VmProp {
    pub obj: u32,
    pub name: String,
    pub ty: u32,
    pub num: f32,
    pub color: u32,
    pub text: Arc<str>,
    /// The names an `enum` Property's number stands for.
    pub options: Vec<Arc<str>>,
}
pub struct BindDef {
    pub node: u32,
    pub prop: u16,
    pub slot: usize,
    pub mul: f32,
    pub add: f32,
    /// What a number is kept within after `mul` and `add`, and how it is made whole
    /// (0 not, 1 the nearest, 2 down, 3 up).
    pub min: f32,
    pub max: f32,
    pub round: u32,
    /// `direction`: 0 to the object, 1 to the view model, 2 both ways.
    pub dir: u32,
    pub format: Option<Arc<str>>,
    pub decimals: u32,
}

/// What a nested artboard plays.
#[derive(Clone, Copy)]
pub enum Play {
    /// Its first state machine, or failing that its first animation.
    Auto,
    Machine(usize),
    Anim(usize),
}
/// A NestedArtboard node, resolved against the artboard it instantiates.
pub struct Nest {
    pub node: u32,
    pub artboard: usize,
    pub play: Play,
    /// (view-model slot here, slot in the nested artboard): written down every frame.
    pub binds: Vec<(usize, usize)>,
    /// Slots in the nested artboard that are given a copy's position in the list.
    pub indexed: Vec<usize>,
}
impl Artboard {
    /// The Runs of Text node `text`, in order.
    pub fn runs(&self, text: u32) -> impl Iterator<Item = u32> + '_ {
        self.nodes[text as usize].children.iter().copied().filter(|c| self.nodes[*c as usize].ty == t::RUN)
    }
}

/// A Skin: `weights[i]` is how firmly the bone holds the i-th point of the path
/// (empty: every point alike).
pub struct Skin {
    pub path: u32,
    pub bone: u32,
    pub weights: Vec<f32>,
}

pub struct Artboard {
    pub obj: u32,
    pub name: String,
    /// Scene nodes in file order; node 0 is the artboard. Parents precede children.
    pub nodes: Vec<NodeDef>,
    /// File index -> node index.
    pub local: HashMap<u32, u32>,
    pub anims: Vec<Anim>,
    pub machines: Vec<Machine>,
    pub vm: Vec<VmProp>,
    pub binds: Vec<BindDef>,
    /// Names of the Events this artboard declares.
    pub events: Vec<String>,
    /// NestedArtboard nodes that point at an artboard, in node order.
    pub nests: Vec<Nest>,
    /// Some node has a Constraint: world transforms take a second pass.
    pub constrained: bool,
    pub skins: Vec<Skin>,
    /// The Text nodes that are set along a Shape's outline (`follow`).
    pub bent: Vec<u32>,
    /// The Layout nodes, in document order (a Layout before the Layouts inside it).
    pub layouts: Vec<u32>,
    /// For each axis of a Joystick that scrubs an animation: the node, the handle's
    /// property (`handleX` or `handleY`) and the animation (an index into `anims`).
    pub joysticks: Vec<(u32, u16, usize)>,
}

pub struct File {
    pub objects: Vec<RawObject>,
    pub artboards: Vec<Artboard>,
    /// Decoded image pixels by ImageAsset file index, supplied by the host.
    pub images: Mutex<HashMap<u32, Arc<Image>>>,
    /// Bumped by `set_image`, so instances that already settled draw the new pixels.
    pub image_gen: AtomicU32,
    pub meshes: HashMap<u32, Arc<Mesh>>,
    /// Something in the file is `opaque`, or can become so (a Bind or a Track of it).
    pub veils: bool,
}

impl File {
    pub fn load(bytes: &[u8]) -> Result<Arc<File>> {
        File::from_objects(format::read(bytes)?)
    }

    pub fn from_objects(objects: Vec<RawObject>) -> Result<Arc<File>> {
        let (children, root) = format::tree(&objects)?;
        let mut meshes = HashMap::new();
        for (i, o) in objects.iter().enumerate() {
            if o.type_key == t::MESH_ASSET && o.parent().is_none() {
                let mesh = Mesh::parse(o.bytes(p::BYTES).unwrap_or(&[]));
                meshes.insert(i as u32, Arc::new(mesh.map_err(|e| Error(format!("{}: {e}", describe(&objects, i))))?));
            }
        }
        let mut artboards = Vec::new();
        for (i, o) in objects.iter().enumerate() {
            if o.type_key == t::ARTBOARD && o.parent().is_none() {
                artboards.push(build_artboard(&objects, &children, &root, i as u32)?);
            }
        }
        // Nested artboards refer to artboards that may come later in the file.
        for i in 0..artboards.len() {
            artboards[i].nests = build_nests(&objects, &children, &artboards, &artboards[i])?;
        }
        let veils = objects.iter().any(|o| o.uint(p::OPAQUE_TARGET) == Some(1) || o.uint(p::PROPERTY) == Some(p::OPAQUE_TARGET as u64));
        Ok(Arc::new(File { objects, artboards, images: Mutex::new(HashMap::new()), image_gen: AtomicU32::new(0), meshes, veils }))
    }

    /// File indices of root objects of type `ty` (assets, artboards).
    pub fn roots_of(&self, ty: u16) -> Vec<u32> {
        (0..self.objects.len() as u32).filter(|i| self.objects[*i as usize].type_key == ty && self.objects[*i as usize].parent().is_none()).collect()
    }

    /// The sound Event `event` plays when it is reported: (its AudioAsset, how loud, 0..1).
    pub fn event_sound(&self, event: u32) -> Option<(u32, f32)> {
        let e = self.objects.get(event as usize).filter(|e| e.type_key == t::EVENT_NODE)?;
        let asset = e.uint(p::ASSET).filter(|a| self.objects.get(*a as usize).is_some_and(|o| o.type_key == t::AUDIO_ASSET))?;
        Some((asset as u32, f(e, p::VOLUME).clamp(0.0, 1.0)))
    }

    /// Give the runtime the decoded pixels of an ImageAsset (straight-alpha RGBA8).
    pub fn set_image(&self, asset: u32, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
        if self.objects.get(asset as usize).map(|o| o.type_key) != Some(t::IMAGE_ASSET) {
            bail!("object {asset} is not an ImageAsset");
        }
        if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
            bail!("image pixels must be {w} x {h} x 4 bytes, got {}", rgba.len());
        }
        let mut px = rgba.to_vec();
        crate::raster::premultiply(&mut px);
        self.images.lock().unwrap_or_else(|e| e.into_inner()).insert(asset, Arc::new(Image { w, h, px }));
        self.image_gen.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

/// "Shape 'ball' (object 12)" for error messages.
pub fn describe(objects: &[RawObject], i: usize) -> String {
    let o = &objects[i];
    let ty = schema::type_def(o.type_key).map_or("object", |t| t.name);
    match o.str(p::NAME) {
        Some(n) => format!("{ty} '{n}' (object {i})"),
        None => format!("{ty} (object {i})"),
    }
}

fn f(o: &RawObject, key: u16) -> f32 {
    o.float(key).filter(|v| v.is_finite()).unwrap_or_else(|| schema::default(key))
}
fn u(o: &RawObject, key: u16) -> u32 {
    o.uint(key).map_or(schema::default(key) as u32, |v| v as u32)
}

fn build_artboard(objects: &[RawObject], children: &[Vec<u32>], root: &[u32], ab: u32) -> Result<Artboard> {
    let o = |i: u32| &objects[i as usize];
    let kids = |i: u32, ty: u16| children[i as usize].iter().copied().filter(move |c| objects[*c as usize].type_key == ty);
    let mut out = Artboard {
        obj: ab,
        name: o(ab).str(p::NAME).unwrap_or("").to_string(),
        nodes: Vec::new(),
        local: HashMap::new(),
        anims: Vec::new(),
        machines: Vec::new(),
        vm: Vec::new(),
        binds: Vec::new(),
        events: kids(ab, t::EVENT_NODE).filter_map(|e| o(e).str(p::NAME)).map(String::from).collect(),
        nests: Vec::new(),
        constrained: false,
        skins: Vec::new(),
        bent: Vec::new(),
        layouts: Vec::new(),
        joysticks: Vec::new(),
    };

    // Scene nodes: file order guarantees parents come first.
    for i in ab..objects.len() as u32 {
        if root[i as usize] != ab || !schema::is_scene(o(i).type_key) {
            continue;
        }
        let parent = match o(i).parent() {
            None => NONE,
            Some(par) => match out.local.get(&(par as u32)) {
                Some(l) => *l,
                None => continue, // under an object this runtime does not know
            },
        };
        let idx = out.nodes.len() as u32;
        if parent != NONE {
            out.nodes[parent as usize].children.push(idx);
            if schema::is_drawable(o(i).type_key) {
                out.nodes[parent as usize].draws.push(idx);
            }
        }
        out.local.insert(i, idx);
        out.constrained |= o(i).type_key == t::CONSTRAINT;
        if o(i).type_key == t::CONSTRAINT && o(i).uint(p::CONSTRAIN) == Some(6) && out.nodes[parent as usize].ty != t::BONE {
            bail!("{}: an `ik` Constraint goes inside the Bone whose far end should reach the target", describe(objects, i as usize));
        }
        out.nodes.push(NodeDef { ty: o(i).type_key, obj: i, parent, children: Vec::new(), draws: Vec::new() });
    }

    out.layouts = (0..out.nodes.len() as u32).filter(|n| out.nodes[*n as usize].ty == t::LAYOUT).collect();
    out.bent = (0..out.nodes.len() as u32).filter(|n| o(out.nodes[*n as usize].obj).uint(p::FOLLOW).is_some()).collect();

    // Skins: each binds a Path to a Bone. No weights = the bone holds every point alike.
    for (path, node) in out.nodes.iter().enumerate().filter(|(_, n)| n.ty == t::PATH) {
        let mut count = None;
        for skin in kids(node.obj, t::SKIN) {
            let points = *count.get_or_insert_with(|| {
                let mut d = parse_svg_path(o(node.obj).str(p::D).unwrap_or("")).unwrap_or_default();
                d.iter_mut().map(|el| el.points().into_iter().flatten().count()).sum::<usize>()
            });
            let held = |w: &Vec<f32>| w.len() == points && w.iter().all(|w| (0.0..=1.0).contains(w));
            let Some(weights) = o(skin).str(p::WEIGHTS).map_or(Some(Vec::new()), |text| geom::numbers(text).filter(held)) else {
                bail!(
                    "{}: `weights` takes one number from 0 to 1 per point of the Path, which has {points} (a curve counts three)",
                    describe(objects, skin as usize)
                );
            };
            // A Skin without a `target` is not bound yet, like a Constraint without one.
            if let Some(&bone) = o(skin).uint(p::TARGET).and_then(|b| out.local.get(&(b as u32))) {
                out.skins.push(Skin { path: path as u32, bone, weights });
            }
        }
    }

    // View-model properties (state machines and binds refer to them by slot).
    let mut slot_of = HashMap::new();
    for vm in kids(ab, t::VIEW_MODEL) {
        for pr in kids(vm, t::PROPERTY_NODE) {
            slot_of.insert(pr, out.vm.len());
            let (ty, mut num, text) = (u(o(pr), p::VALUE_TYPE), f(o(pr), p::VALUE), o(pr).str(p::TEXT_VALUE));
            // Only an enum has Options, and it starts on one of them: by name or by position.
            let mut options: Vec<Arc<str>> = Vec::new();
            if ty == 5 {
                options = kids(pr, t::OPTION).map(|opt| o(opt).str(p::NAME).unwrap_or("").into()).collect();
                num = text.map_or(num, |name| options.iter().position(|opt| &**opt == name).map_or(-1.0, |i| i as f32));
                if !(num.fract() == 0.0 && num >= 0.0 && (num as usize) < options.len()) {
                    bail!("{}: an enum's value must be one of its Options, by name or by position", describe(objects, pr as usize));
                }
            }
            out.vm.push(VmProp {
                obj: pr,
                name: o(pr).str(p::NAME).unwrap_or("").to_string(),
                ty,
                num,
                color: o(pr).color(p::COLOR_VALUE).unwrap_or(schema::DEFAULT_COLOR),
                text: text.unwrap_or("").into(),
                options,
            });
        }
    }
    let slot = |owner: u32, key: u16| -> Result<Option<usize>> {
        match o(owner).uint(key) {
            None => Ok(None),
            Some(r) => match slot_of.get(&(r as u32)) {
                Some(s) => Ok(Some(*s)),
                None => {
                    // The player build has no property names: it gives the key.
                    #[cfg(feature = "json")]
                    let what = schema::prop(key).map_or("?", |p| p.name);
                    #[cfg(not(feature = "json"))]
                    let what = key;
                    bail!("{}: '{what}' must refer to a view-model Property", describe(objects, owner as usize))
                }
            },
        }
    };

    let mut anim_of = HashMap::new();
    for a in kids(ab, t::ANIMATION) {
        let mut tracks = Vec::new();
        for tr in kids(a, t::TRACK) {
            let (Some(target), Some(def)) = (o(tr).uint(p::TARGET), o(tr).uint(p::PROPERTY).and_then(|k| schema::prop(k as u16))) else { continue };
            let Some(node) = out.local.get(&(target as u32)).copied() else {
                bail!("{}: track target is not a drawable object of this artboard", describe(objects, a as usize));
            };
            let mut order: Vec<(f32, u32)> = kids(tr, t::KEY).map(|k| (f(o(k), p::FRAME), k)).collect();
            sort_keyed(&mut order);
            let mut keys = Vec::new();
            for (frame, k) in order {
                let ko = o(k);
                let val = match def.kind {
                    Kind::Float => PVal::F(f(ko, p::VALUE)),
                    Kind::Color => PVal::C(ko.color(p::COLOR_VALUE).unwrap_or(schema::DEFAULT_COLOR)),
                    Kind::Str if def.key == p::D => PVal::P(Arc::new(
                        parse_svg_path(ko.str(p::TEXT_VALUE).unwrap_or("")).map_err(|e| Error(format!("{}: {e}", describe(objects, k as usize))))?,
                    )),
                    Kind::Str => PVal::S(ko.str(p::TEXT_VALUE).unwrap_or("").into()),
                    _ => PVal::U(f(ko, p::VALUE).max(0.0) as u32),
                };
                let interp = u(ko, p::INTERP);
                let ease = if interp >= 3 {
                    // Elastic: `turn` is the angle per unit of the curve, which for
                    // `elasticInOut` runs twice in the segment; `shift` makes it start at 0.
                    let (reach, half) = (f(ko, p::AMPLITUDE).max(1.0), if interp == 5 { 0.5 } else { 1.0 });
                    let turn = std::f32::consts::TAU * half / f(ko, p::PERIOD).max(0.05);
                    [reach, turn, (1.0 / reach).asin() / turn, 0.0]
                } else {
                    [f(ko, p::EASE_X1), f(ko, p::EASE_Y1), f(ko, p::EASE_X2), f(ko, p::EASE_Y2)]
                };
                keys.push(Key { frame, val, interp, ease });
            }
            tracks.push(Track { node, prop: def.key, keys });
        }
        anim_of.insert(a, out.anims.len());
        out.anims.push(Anim {
            name: o(a).str(p::NAME).unwrap_or("").to_string(),
            fps: f(o(a), p::FPS).max(1e-3),
            duration: f(o(a), p::DURATION).max(0.0),
            looping: u(o(a), p::LOOP),
            speed: f(o(a), p::SPEED),
            tracks,
            cues: kids(a, t::CUE).filter_map(|c| Some((f(o(c), p::FRAME), o(c).uint(p::EMIT)? as u32))).collect(),
        });
    }
    let anim = |owner: u32, key: u16| o(owner).uint(key).and_then(|r| anim_of.get(&(r as u32)).copied());
    for (n, node) in out.nodes.iter().enumerate().filter(|(_, node)| node.ty == t::JOYSTICK) {
        for (handle, key) in [(p::HANDLE_X, p::X_ANIMATION), (p::HANDLE_Y, p::Y_ANIMATION)] {
            let Some(a) = anim(node.obj, key) else { continue };
            // A handle moved by what a handle poses would chase itself from frame to frame.
            if out.anims[a].tracks.iter().any(|tr| out.nodes[tr.node as usize].ty == t::JOYSTICK) {
                bail!("{}: the animation a Joystick scrubs cannot move a Joystick's handle", describe(objects, node.obj as usize));
            }
            out.joysticks.push((n as u32, handle, a));
        }
    }
    // The changes an object makes when it fires: its own `input` / `action` (a Listener's), then its Action children.
    let acts = |owner: u32| -> Result<Vec<Act>> {
        let mut list = Vec::new();
        for a in std::iter::once(owner).chain(kids(owner, t::ACTION_NODE)) {
            let Some(slot) = slot(a, p::INPUT)? else {
                if a == owner {
                    continue; // the Listener or Transition itself: its own `input` is optional
                }
                bail!("{}: an Action needs an 'input'", describe(objects, a as usize));
            };
            let action = u(o(a), p::ACTION);
            // `fire` is for triggers; set, toggle and add are for numbers, bools and enums.
            let ty = out.vm[slot].ty;
            if (action == 2) != (ty == 2) || matches!(ty, 3 | 4) {
                bail!("{}: 'action' cannot be applied to a Property of that type", describe(objects, a as usize));
            }
            list.push(Act { slot, action, value: f(o(a), p::VALUE) });
        }
        Ok(list)
    };

    for m in kids(ab, t::STATE_MACHINE) {
        let mut layers = Vec::new();
        for l in kids(m, t::LAYER) {
            let state_objs: Vec<u32> = kids(l, t::STATE).collect();
            let mut states = Vec::new();
            for &s in &state_objs {
                let mut transitions = Vec::new();
                for tr in kids(s, t::TRANSITION) {
                    let to = o(tr).uint(p::TO).and_then(|r| state_objs.iter().position(|x| *x as u64 == r));
                    let Some(to) = to else { bail!("{}: a Transition's 'to' must name a State in the same Layer", describe(objects, s as usize)) };
                    let mut conds = Vec::new();
                    for c in kids(tr, t::CONDITION) {
                        let Some(input) = slot(c, p::INPUT)? else { bail!("{}: a Condition needs an 'input'", describe(objects, s as usize)) };
                        let with = slot(c, p::WITH)?;
                        if with.is_some_and(|w| !matches!(out.vm[w].ty, 0 | 1 | 5)) {
                            bail!("{}: a Condition's 'with' must be a number, bool or enum Property", describe(objects, s as usize));
                        }
                        conds.push(Cond { slot: input, op: u(o(c), p::OP), value: f(o(c), p::VALUE), with });
                    }
                    transitions.push(Transition {
                        to,
                        blend: f(o(tr), p::BLEND).max(0.0),
                        exit_time: f(o(tr), p::EXIT_TIME),
                        conds,
                        emit: o(tr).uint(p::EMIT).map(|e| e as u32),
                        interp: u(o(tr), p::INTERP),
                        ease: [f(o(tr), p::EASE_X1), f(o(tr), p::EASE_Y1), f(o(tr), p::EASE_X2), f(o(tr), p::EASE_Y2)],
                        pause: u(o(tr), p::PAUSE) != 0,
                        chance: f(o(tr), p::CHANCE),
                        acts: acts(tr)?,
                    });
                }
                let kind = u(o(s), p::KIND);
                let mut blend = Vec::new();
                for b in kids(s, t::BLEND_ANIMATION) {
                    blend.extend(anim(b, p::ANIMATION_REF).map(|a| Ok((a, f(o(b), p::VALUE), slot(b, p::INPUT)?))).transpose()?);
                }
                if kind == 4 {
                    let mut order: Vec<(f32, u32)> = blend.iter().enumerate().map(|(i, b)| (b.1, i as u32)).collect();
                    sort_keyed(&mut order);
                    blend = order.iter().map(|(_, i)| blend[*i as usize]).collect();
                }
                states.push(State { obj: s, kind, anim: anim(s, p::ANIMATION_REF), speed: f(o(s), p::SPEED), input: slot(s, p::INPUT)?, blend, transitions });
            }
            let entry = states.iter().position(|s| s.kind == 1).unwrap_or(0);
            let any = (0..states.len()).filter(|i| states[*i].kind == 2).collect();
            layers.push(Layer { states, entry, any });
        }
        let mut listeners = Vec::new();
        for l in kids(m, t::LISTENER) {
            let target = match o(l).uint(p::TARGET) {
                None => None,
                Some(r) => match out.local.get(&(r as u32)) {
                    Some(n) => Some(*n),
                    None => bail!("{}: 'target' must be a drawable object of this artboard", describe(objects, l as usize)),
                },
            };
            let (event, key) = (u(o(l), p::EVENT), o(l).str(p::KEY_NAME));
            let watch = slot(l, p::WITH)?.filter(|_| event == EVENT_CHANGE);
            if event == EVENT_CHANGE && watch.is_none() {
                bail!("{}: a `change` Listener needs 'with': the Property to watch", describe(objects, l as usize));
            }
            // A key Listener hears the whole artboard's keys; only it has a `key`.
            let keyed = matches!(event, EVENT_KEY_DOWN | EVENT_KEY_UP);
            if if keyed { target.is_some() || key == Some("") } else { key.is_some() } {
                bail!("{}: 'key' (not empty) is for a keyDown or keyUp Listener, and those take no 'target'", describe(objects, l as usize));
            }
            listeners.push(Listener { target, watch, key: key.map(Arc::from), event, acts: acts(l)?, emit: o(l).uint(p::EMIT).map(|e| e as u32) });
        }
        out.machines.push(Machine { name: o(m).str(p::NAME).unwrap_or("").to_string(), layers, listeners });
    }

    for (idx, node) in out.nodes.iter().enumerate() {
        for b in kids(node.obj, t::BIND) {
            if o(b).str(p::INTO).is_some() && node.ty != t::NESTED_ARTBOARD {
                bail!("{}: 'into' is for a Bind inside a NestedArtboard", describe(objects, b as usize));
            }
            let (Some(slot), Some(prop)) = (slot(b, p::SOURCE)?, o(b).uint(p::PROPERTY)) else { continue };
            // A Bind that writes back needs a Property that can hold what the object holds.
            let (holds, kind) = match schema::prop(prop as u16).map(|d| d.kind) {
                Some(Kind::Color) => (3, "color"),
                Some(Kind::Str) => (4, "string"),
                _ => (0, "number or bool"),
            };
            let dir = u(o(b), p::DIRECTION);
            if dir != 0 && out.vm[slot].ty != holds && !(holds == 0 && out.vm[slot].ty == 1) {
                bail!("{}: a Bind that writes to its source needs a {kind} Property for this property", describe(objects, b as usize));
            }
            // A limit as far off as the default (a billion either way) is none: a bound
            // number can be larger than that. Where they cross, `max` wins.
            let limit = |key: u16| {
                let v = f(o(b), key);
                if v.abs() >= 1e9 {
                    v * f32::INFINITY
                } else {
                    v
                }
            };
            out.binds.push(BindDef {
                node: idx as u32,
                prop: prop as u16,
                slot,
                mul: f(o(b), p::MUL),
                add: f(o(b), p::ADD),
                min: limit(p::MIN).min(limit(p::MAX)),
                max: limit(p::MAX),
                round: u(o(b), p::ROUND),
                dir,
                format: o(b).str(p::FORMAT).map(Arc::from),
                decimals: u(o(b), p::DECIMALS).min(9),
            });
        }
    }
    Ok(out)
}

/// Resolve the NestedArtboard nodes of `ab`: which artboard, what it plays, what data goes in.
fn build_nests(objects: &[RawObject], children: &[Vec<u32>], artboards: &[Artboard], ab: &Artboard) -> Result<Vec<Nest>> {
    let mut nests = Vec::new();
    for (node, n) in ab.nodes.iter().enumerate().filter(|(_, n)| n.ty == t::NESTED_ARTBOARD) {
        let (o, here) = (&objects[n.obj as usize], describe(objects, n.obj as usize));
        let binds_into = |b: &&u32| objects[**b as usize].type_key == t::BIND && objects[**b as usize].str(p::INTO).is_some();
        let Some(artboard) = o.uint(p::ARTBOARD_REF).and_then(|r| artboards.iter().position(|a| a.obj as u64 == r)) else {
            // A reference that is missing or points elsewhere draws nothing; data for it is a mistake.
            if children[n.obj as usize].iter().any(|b| binds_into(&b)) {
                bail!("{here}: a Bind with 'into' needs the NestedArtboard to name an 'artboard'");
            }
            continue;
        };
        let child = &artboards[artboard];
        let play = match o.str(p::PLAY) {
            None | Some("") => Play::Auto,
            Some(name) => match (child.machines.iter().position(|m| m.name == name), child.anims.iter().position(|a| a.name == name)) {
                (Some(m), _) => Play::Machine(m),
                (None, Some(a)) => Play::Anim(a),
                _ => bail!("{here}: artboard '{}' has no StateMachine or Animation named '{name}'", child.name),
            },
        };
        let (mut binds, mut indexed) = (Vec::new(), Vec::new());
        for &b in children[n.obj as usize].iter().filter(binds_into) {
            let bo = &objects[b as usize];
            let into = bo.str(p::INTO).unwrap_or("");
            let Some(to) = child.vm.iter().position(|v| v.name == into) else {
                bail!("{here}: artboard '{}' has no view-model Property named '{into}'", child.name)
            };
            // One Bind writes a value: a second would only be overwritten, or overwrite.
            // (A trigger is fired by any of those bound into it.)
            if child.vm[to].ty != 2 && (indexed.contains(&to) || binds.iter().any(|b: &(usize, usize)| b.1 == to)) {
                bail!("{here}: two Binds write into '{into}'");
            }
            // The copy's position in the list, in place of a Property of this artboard.
            if u(bo, p::INDEX) != 0 {
                if child.vm[to].ty != 0 {
                    bail!("{here}: a Bind with 'index' writes a position into a number Property");
                }
                indexed.push(to);
                continue;
            }
            let Some(from) = bo.uint(p::SOURCE).and_then(|r| ab.vm.iter().position(|v| v.obj as u64 == r)) else {
                bail!("{here}: a Bind with 'into' needs a 'source' view-model Property of this artboard, or 'index'")
            };
            if ab.vm[from].ty != child.vm[to].ty || ab.vm[from].options != child.vm[to].options {
                bail!(
                    "{here}: '{}' and '{into}' of artboard '{}' are view-model Properties of different types (or enums with different Options)",
                    ab.vm[from].name,
                    child.name
                );
            }
            binds.push((from, to));
        }
        nests.push(Nest { node: node as u32, artboard, play, binds, indexed });
    }
    Ok(nests)
}
