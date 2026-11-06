//! Keeping a meeting: the files on disk, and the live state a window edits.
//! Files stay compatible with the older app, and anything this version does not understand survives
//! a save untouched. Free-edit scenes are kept but cannot be edited, as free editing is gone.

mod live;
mod store;

pub use live::{History, Live, Snap, ViewOnly, UNTITLED};
pub use store::{decode_data_url, default_dir, new_id, Info, Loaded, Origin, PageImage, Store};
