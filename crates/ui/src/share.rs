//! Showing the open meeting to others on the network, while sharing is on: the server, and a thread that
//! lays the boards out for it on its own pen, so the board on screen never waits for viewers.

use anyhow::Result;
use boardcast_board::glyphs::{Glyphs, HanziSource};
use boardcast_export::{replay_page, words_and_boards};
use boardcast_meetings::{Live, PageImage, Store};
use boardcast_share::Share;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// However fast the meeting changes, viewers are sent it at most this often.
const EVERY: Duration = Duration::from_millis(400);

/// The meeting as it is now, for the thread to lay out.
struct Now {
    doc: boardcast_export::Doc,
    pages: Vec<replay_page::Page>,
    current: usize,
}

pub struct Sharing {
    share: Arc<Share>,
    /// The meeting being shown; only ever this one.
    meeting: String,
    to_layout: Sender<Now>,
    /// What was last sent, to send again only when the meeting changed: the meeting, its changes, how many
    /// lines it has, the board it is on, and whether its summary is there.
    sent: Option<(String, u64, usize, usize, usize)>,
    sent_at: Instant,
    /// Each page image the meeting keeps, by its name, read from disk once.
    images: HashMap<String, Option<PageImage>>,
}

impl Sharing {
    /// Show `meeting` on the network.
    pub fn start(meeting: &str, hanzi: HanziSource) -> Result<Self> {
        let share = Arc::new(Share::start()?);
        let (to_layout, from_app) = channel::<Now>();
        let shown = share.clone();
        std::thread::Builder::new().name("share-layout".into()).spawn(move || {
            let mut glyphs = match Glyphs::load(hanzi) {
                Ok(glyphs) => glyphs,
                Err(e) => return eprintln!("sharing: no pen for the boards: {e:#}"),
            };
            let mut live = replay_page::Live::default();
            while let Ok(mut now) = from_app.recv() {
                // Only the latest matters: anything sent while the last was laid out is already out of date.
                while let Ok(newer) = from_app.try_recv() {
                    now = newer;
                }
                let (data, images) = live.data(&now.doc, &now.pages, now.current, &mut glyphs);
                let page = replay_page::live_page(&now.doc, &data);
                shown.show(page, data, images);
            }
        })?;
        Ok(Self { share, meeting: meeting.to_string(), to_layout, sent: None, sent_at: Instant::now() - EVERY, images: HashMap::new() })
    }

    pub fn share(&self) -> &Share {
        &self.share
    }

    /// Whether this is the meeting being shown. Another one opened is never shown in its place.
    pub fn shows(&self, live: &Live) -> bool {
        live.id() == self.meeting
    }

    /// Send the meeting to viewers if it changed since it was last sent and that was long enough ago.
    pub fn offer(&mut self, live: &Live, store: Option<&Store>, english: bool) {
        if !self.shows(live) {
            return;
        }
        let now = (live.id().to_string(), live.changes, live.lines.len(), live.page(), live.summary().len());
        if self.sent.as_ref() == Some(&now) || self.sent_at.elapsed() < EVERY {
            return;
        }
        let id = live.id().to_string();
        let read = |name: &str| match store {
            Some(store) => store.page_image(&id, name).ok(),
            None => boardcast_meetings::decode_data_url(name).ok(),
        };
        let refs = live.board_images();
        for name in refs.iter().flatten() {
            if !self.images.contains_key(name) {
                self.images.insert(name.clone(), read(name));
            }
        }
        let images: Vec<Option<PageImage>> = refs.iter().map(|r| r.as_ref().and_then(|name| self.images.get(name).cloned().flatten())).collect();
        let (doc, pages) = words_and_boards(live, english, &images, &|name| read(name));
        let _ = self.to_layout.send(Now { doc, pages, current: live.page() });
        self.sent = Some(now);
        self.sent_at = Instant::now();
    }
}

/// The QR code of a link, as rows of dark and light modules, quiet zone left to the caller.
pub fn qr(link: &str) -> Option<Vec<Vec<bool>>> {
    let code = qrcode::QrCode::with_error_correction_level(link.as_bytes(), qrcode::EcLevel::M).ok()?;
    let width = code.width();
    let modules = code.to_colors();
    Some(modules.chunks(width).map(|row| row.iter().map(|c| *c == qrcode::Color::Dark).collect()).collect())
}
