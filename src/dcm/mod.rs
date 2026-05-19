//! DICOM domain layer: study/series/instance model, folder loading, and
//! pixel-data decoding to RGBA for the viewport.

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
