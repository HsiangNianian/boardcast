//! Keeping a meeting: the files on disk, and the live state a window edits.
//! Files stay compatible with the older app, and anything this version does not understand survives
//! a save untouched. Imported pages and free-edit scenes are shown but not edited yet.

mod live;
mod store;

pub use live::{History, Live, Snap, ViewOnly, UNTITLED};
pub use store::{default_dir, new_id, Info, Loaded, Origin, Store};
