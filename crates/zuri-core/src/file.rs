//! A loaded `.zur` file: the raw object tree plus everything that can be compiled once and
//! shared by all instances (scene topology, animations, state machines, view-model layout).

use crate::format::{self, RawObject};
use crate::geom::{parse_svg_path, Path};
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
}

pub struct Key {
    pub frame: f32,
    pub val: PVal,
    pub interp: u32,
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
}

pub struct Cond {
    pub slot: usize,
    pub op: u32,
    pub value: f32,
}
pub struct Transition {
    pub to: usize,
    pub blend: f32,
    pub exit_time: f32,
    pub conds: Vec<Cond>,
    pub emit: Option<u32>,
}
pub struct State {
    pub kind: u32,
    pub anim: Option<usize>,
    pub speed: f32,
    pub input: Option<usize>,
    /// (animation, position on the input axis), sorted by position.
    pub blend: Vec<(usize, f32)>,
    pub transitions: Vec<Transition>,
}
pub struct Layer {
    pub states: Vec<State>,
    pub entry: usize,
    pub any: Vec<usize>,
}
pub struct Listener {
    pub target: Option<u32>,
    pub event: u32,
    pub slot: Option<usize>,
    pub action: u32,
    pub value: f32,
    pub emit: Option<u32>,
}
pub struct Machine {
    pub name: String,
    pub layers: Vec<Layer>,
    pub listeners: Vec<Listener>,
}

pub struct VmProp {
    pub name: String,
    pub ty: u32,
    pub num: f32,
    pub color: u32,
    pub text: Arc<str>,
}
pub struct BindDef {
    pub node: u32,
    pub prop: u16,
    pub slot: usize,
    pub mul: f32,
    pub add: f32,
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
}

pub struct File {
    pub objects: Vec<RawObject>,
    pub artboards: Vec<Artboard>,
    /// Decoded image pixels by ImageAsset file index, supplied by the host.
    pub images: Mutex<HashMap<u32, Arc<Image>>>,
    /// Bumped by `set_image`, so instances that already settled draw the new pixels.
    pub image_gen: AtomicU32,
    pub meshes: HashMap<u32, Arc<Mesh>>,
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
        Ok(Arc::new(File { objects, artboards, images: Mutex::new(HashMap::new()), image_gen: AtomicU32::new(0), meshes }))
    }

    pub fn artboard_index(&self, obj: u32) -> Option<usize> {
        self.artboards.iter().position(|a| a.obj == obj)
    }

    /// File indices of root objects of type `ty` (assets, artboards).
    pub fn roots_of(&self, ty: u16) -> Vec<u32> {
        (0..self.objects.len() as u32).filter(|i| self.objects[*i as usize].type_key == ty && self.objects[*i as usize].parent().is_none()).collect()
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
        }
        out.local.insert(i, idx);
        out.nodes.push(NodeDef { ty: o(i).type_key, obj: i, parent, children: Vec::new() });
    }

    // View-model properties (state machines and binds refer to them by slot).
    let mut slot_of = HashMap::new();
    for vm in kids(ab, t::VIEW_MODEL) {
        for pr in kids(vm, t::PROPERTY_NODE) {
            slot_of.insert(pr, out.vm.len());
            out.vm.push(VmProp {
                name: o(pr).str(p::NAME).unwrap_or("").to_string(),
                ty: u(o(pr), p::VALUE_TYPE),
                num: f(o(pr), p::VALUE),
                color: o(pr).color(p::COLOR_VALUE).unwrap_or(schema::DEFAULT_COLOR),
                text: o(pr).str(p::TEXT_VALUE).unwrap_or("").into(),
            });
        }
    }
    let slot = |owner: u32, key: u16| -> Result<Option<usize>> {
        match o(owner).uint(key) {
            None => Ok(None),
            Some(r) => match slot_of.get(&(r as u32)) {
                Some(s) => Ok(Some(*s)),
                None => bail!("{}: '{}' must refer to a view-model Property", describe(objects, owner as usize), schema::prop(key).map_or("?", |p| p.name)),
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
            let mut keys = Vec::new();
            for k in kids(tr, t::KEY) {
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
                keys.push(Key {
                    frame: f(ko, p::FRAME),
                    val,
                    interp: u(ko, p::INTERP),
                    ease: [f(ko, p::EASE_X1), f(ko, p::EASE_Y1), f(ko, p::EASE_X2), f(ko, p::EASE_Y2)],
                });
            }
            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
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
        });
    }
    let anim = |owner: u32| o(owner).uint(p::ANIMATION_REF).and_then(|r| anim_of.get(&(r as u32)).copied());

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
                        let Some(slot) = slot(c, p::INPUT)? else { bail!("{}: a Condition needs an 'input'", describe(objects, s as usize)) };
                        conds.push(Cond { slot, op: u(o(c), p::OP), value: f(o(c), p::VALUE) });
                    }
                    transitions.push(Transition {
                        to,
                        blend: f(o(tr), p::BLEND).max(0.0),
                        exit_time: f(o(tr), p::EXIT_TIME),
                        conds,
                        emit: o(tr).uint(p::EMIT).map(|e| e as u32),
                    });
                }
                let mut blend: Vec<(usize, f32)> = kids(s, t::BLEND_ANIMATION).filter_map(|b| anim(b).map(|a| (a, f(o(b), p::VALUE)))).collect();
                blend.sort_by(|a, b| a.1.total_cmp(&b.1));
                states.push(State { kind: u(o(s), p::KIND), anim: anim(s), speed: f(o(s), p::SPEED), input: slot(s, p::INPUT)?, blend, transitions });
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
            listeners.push(Listener {
                target,
                event: u(o(l), p::EVENT),
                slot: slot(l, p::INPUT)?,
                action: u(o(l), p::ACTION),
                value: f(o(l), p::VALUE),
                emit: o(l).uint(p::EMIT).map(|e| e as u32),
            });
        }
        out.machines.push(Machine { name: o(m).str(p::NAME).unwrap_or("").to_string(), layers, listeners });
    }

    for (idx, node) in out.nodes.iter().enumerate() {
        for b in kids(node.obj, t::BIND) {
            let (Some(slot), Some(prop)) = (slot(b, p::SOURCE)?, o(b).uint(p::PROPERTY)) else { continue };
            out.binds.push(BindDef { node: idx as u32, prop: prop as u16, slot, mul: f(o(b), p::MUL), add: f(o(b), p::ADD) });
        }
    }
    Ok(out)
}
