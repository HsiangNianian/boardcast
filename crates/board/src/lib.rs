//! The board itself: state, the model's operations, layout, pen strokes and the layout check.
//! Pure logic with no UI, audio or network, so every other crate can build on it.

pub mod board_layout;
pub mod check;
pub mod compose;
pub mod glyphs;
pub mod graph;
pub mod pen;
pub mod sugiyama;
