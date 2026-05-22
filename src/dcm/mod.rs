//! DICOM domain layer: study/series/instance model, folder loading, and
//! pixel-data decoding to RGBA for the viewport.
//!
//! The dependency graph inside this layer is roughly linear:
//!
//! ```text
//! loader  →  study  →  pixel  →  roi   →  annotation
//!                                ↘         ↘
//!                                 thumbnail   export
//!                                            ↘
//!                                          metadata
//! ```
//!
//! - [`loader`] walks a folder and produces metadata-only [`Study`] trees.
//! - [`pixel`] decodes a single instance to interactive `f32` values and
//!   re-windows them to RGBA8 on demand.
//! - [`roi`] computes statistics over windows on [`pixel::RawImage`].
//! - [`annotation`] persists user measurements to a JSON sidecar.
//! - [`thumbnail`] decodes a heavily-decimated preview for the sidebar.
//! - [`metadata`] flattens DICOM tags for the right-hand panel.
//! - [`export`] writes annotated PNGs and a basic anonymised copy of the
//!   source DICOM.

pub mod annotation;
pub mod export;
pub mod loader;
pub mod metadata;
pub mod pixel;
pub mod roi;
pub mod study;
pub mod thumbnail;

#[allow(unused_imports)]
pub use pixel::DecodedFrame;
pub use pixel::{auto_window, decode_to_rgba, load_raw, render_rgba, RawImage, WindowSetting};
pub use study::{Instance, Series, Study};
