//! The inbox: cards tucked away at the left edge of the board, kept by the app rather than by a meeting, so
//! they follow from one meeting to the next. Each card keeps its own picture, so it stays whole whatever
//! happens to the meeting it came from.

use crate::store::{mime_of, PageImage};
use anyhow::{Context, Result};
use boardcast_board::graph::Annotation;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// A card in the inbox: what it says, its size on a board, and what was drawn on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tucked {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub text: String,
    /// The card's part of the page, in board units: the clip it becomes again is cut from all of its picture.
    pub w: f64,
    pub h: f64,
    /// The picture's file in the inbox's folder.
    pub image: String,
    #[serde(default)]
    pub marks: Vec<String>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// The meeting it was taken from, by title.
    #[serde(default)]
    pub from: String,
}

pub struct Inbox {
    dir: PathBuf,
}

impl Inbox {
    /// The inbox of the app whose meetings are kept under `root`.
    pub fn new(root: PathBuf) -> Self {
        Self { dir: root.join("inbox") }
    }

    fn list_path(&self) -> PathBuf {
        self.dir.join("inbox.json")
    }

    /// The cards, oldest first. A missing or unreadable list is an empty inbox.
    pub fn cards(&self) -> Vec<Tucked> {
        fs::read(self.list_path()).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    fn keep(&self, cards: &[Tucked]) -> Result<()> {
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        let tmp = self.list_path().with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(cards)?).with_context(|| format!("saving {}", tmp.display()))?;
        fs::rename(&tmp, self.list_path()).context("saving the inbox")
    }

    /// Put a card in, with its picture; `card.id` and `card.image` are given here.
    pub fn tuck(&self, mut card: Tucked, mime: &str, bytes: &[u8]) -> Result<Tucked> {
        let extension = ["png", "jpg", "webp", "gif"].into_iter().find(|e| mime_of(&format!("x.{e}")) == Some(mime)).with_context(|| format!("unsupported image type {mime}"))?;
        let mut cards = self.cards();
        card.id = crate::new_id();
        while cards.iter().any(|c| c.id == card.id) {
            card.id.push('x');
        }
        card.image = format!("{}.{extension}", card.id);
        fs::create_dir_all(&self.dir).with_context(|| format!("creating {}", self.dir.display()))?;
        fs::write(self.dir.join(&card.image), bytes).with_context(|| format!("saving {}", card.image))?;
        cards.push(card.clone());
        self.keep(&cards)?;
        Ok(card)
    }

    /// Take a card out of the inbox for good.
    pub fn remove(&self, id: &str) -> Result<()> {
        let mut cards = self.cards();
        let Some(i) = cards.iter().position(|c| c.id == id) else { return Ok(()) };
        let card = cards.remove(i);
        self.keep(&cards)?;
        let _ = fs::remove_file(self.dir.join(card.image));
        Ok(())
    }

    /// A card's picture.
    pub fn image(&self, card: &Tucked) -> Result<PageImage> {
        // The name is ours, but a list edited by hand must not reach outside the folder.
        if card.image.contains(['/', '\\']) || card.image.starts_with('.') {
            anyhow::bail!("not an inbox picture: {}", card.image);
        }
        let mime = mime_of(&card.image).context("unknown image type")?.to_string();
        let bytes = fs::read(self.dir.join(&card.image)).with_context(|| format!("reading {}", card.image))?;
        Ok(PageImage { mime, bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_is_kept_with_its_picture_and_can_be_taken_out() {
        let root = std::env::temp_dir().join(format!("boardcast-inbox-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let inbox = Inbox::new(root.clone());
        assert!(inbox.cards().is_empty());
        let card = Tucked { id: String::new(), label: "Table 4".into(), text: "Batch-8 41ms".into(), w: 400.0, h: 90.0, image: String::new(), marks: vec!["星标".into()], annotations: vec![], from: "Review".into() };
        let kept = inbox.tuck(card.clone(), "image/png", b"png bytes").unwrap();
        let again = Inbox::new(root.clone());
        assert_eq!(again.cards(), vec![kept.clone()], "it outlasts the app");
        assert_eq!(again.image(&kept).unwrap().bytes, b"png bytes");
        let second = inbox.tuck(card, "image/png", b"other").unwrap();
        assert_ne!(second.id, kept.id);
        inbox.remove(&kept.id).unwrap();
        assert_eq!(inbox.cards(), vec![second]);
        assert!(!root.join("inbox").join(&kept.image).exists(), "its picture goes with it");
        assert!(inbox.image(&Tucked { image: "../secret.png".into(), ..kept }).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
