//! Audio capture and processing module

mod capture;
mod encoder;

pub use capture::{AudioCapture, INPUT_LEVEL};
pub use encoder::OpusEncoder;
