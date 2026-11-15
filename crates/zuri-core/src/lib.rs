//! Zuri runtime core.
//!
//! Layers, bottom to top (each only depends on the ones above it in this list):
//! - [`format`]   `.zur` binary container (header, ToC, flat object stream)
//! - [`schema`]   the single source of truth for object types and properties
//! - [`geom`]     2D math, paths, flattening, stroking
//! - [`raster`]   reference CPU rasterizer / compositor (premultiplied RGBA8)
//! - [`file`]     loaded file: object tree + compiled animation / state-machine defs
//! - [`instance`] a playable artboard instance: advance, data binding, pointer, render
//! - [`json`]     JSON source twin (feature `json`, authoring only)

macro_rules! bail {
    ($($t:tt)*) => { return Err($crate::Error(format!($($t)*))) };
}

pub mod file;
pub mod format;
pub mod geom;
pub mod instance;
#[cfg(feature = "json")]
pub mod json;
pub mod model;
pub mod raster;
pub mod schema;
#[cfg(feature = "text")]
pub mod text;

mod draw;

pub use file::File;
pub use instance::{Fit, Instance, Pointer};
pub use raster::Raster;

/// The one error type: a human (and agent) readable message.
#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
