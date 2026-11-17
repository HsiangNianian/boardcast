//! Keeping a meeting: the files on disk, and the live state a window edits.
//! Files stay compatible with the older app, and anything this version does not understand survives
//! a save untouched. Free-edit scenes are kept but cannot be edited, as free editing is gone.

mod inbox;
mod live;
mod store;

pub use inbox::{Inbox, Tucked};
pub use live::{History, Live, Note, Snap, ViewOnly, UNTITLED};
pub use store::{decode_data_url, default_dir, new_id, speakers_of, Info, Loaded, Origin, PageImage, Store};
