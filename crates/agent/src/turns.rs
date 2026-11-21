//! What the board hears: whole turns rather than the recognizer's pieces.
//!
//! The recognizer ends a sentence at a short pause, and cuts a long one at a dip in loudness, so one person's
//! turn reaches the board as three or four half sentences ("对，而且你现在设计的产品。" "主要推出的是它有照相的
//! 功能，功能的话是很。" "很很好…"), each drawn on its own as best the model can guess. In the AliMeeting phone
//! meeting played through the app, 49 of 99 lines followed a line by the same speaker. The transcript keeps
//! every piece as it was heard; only what the board model is asked to draw is put back together here:
//!
//! - a speaker's next piece, after a pause shorter than [`JOIN_GAP`], continues their turn;
//! - a short remark by someone else in the middle ("对啊", "Yeah.") is kept for the discussion the model
//!   reads, but does not end the turn it fell into;
//! - a line with nothing to draw (a filler, a stray letter, punctuation) is never a turn of its own;
//! - a turn is handed on when someone else takes over, after a pause, when it grows past [`MAX_CHARS`], or at
//!   once when it points at the board or was typed, as those are meant to be drawn now.
//!
//! The waiting itself, how long to hold a turn while its speaker may go on, belongs to the caller: the app
//! counts quiet time on the clock, an evaluation the pauses in the recording.

use alcyo_board::graph::Utterance;

/// A pause, in seconds of the recording, past which the same speaker starts a new turn.
pub const JOIN_GAP: f64 = 1.2;
/// A turn this long, in characters, is handed on without waiting for the speaker to stop.
pub const MAX_CHARS: usize = 160;
/// Someone else's remark this short, in letters and digits, falls into the turn it interrupts.
const ASIDE_CHARS: usize = 6;

/// A piece of speech as the recognizer heard it, or a line typed in.
pub trait Piece {
    fn utterance(&self) -> &Utterance;
    /// Where in the recording it began and ended, in seconds, when it was heard rather than typed.
    fn span(&self) -> Option<(f64, f64)>;
    /// To be drawn at once: typed, or pointing at the board.
    fn urgent(&self) -> bool;
}

/// What the board is asked about next: a turn to draw, with every piece that went into it or was said
/// during it, in the order heard. A remark with nothing to draw has no turn; it only joins the discussion.
pub struct Ready<P> {
    pub turn: Option<Utterance>,
    pub pieces: Vec<P>,
}

pub struct Assembler<P> {
    /// The turn being put together, with the pieces said during it.
    pending: Vec<P>,
    /// Which of them are the turn's own words.
    own: Vec<bool>,
    /// Where the turn's last own piece ended in the recording.
    last_end: Option<f64>,
}

impl<P> Default for Assembler<P> {
    fn default() -> Self {
        Self { pending: vec![], own: vec![], last_end: None }
    }
}

fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphanumeric()).count()
}

/// A line with nothing to draw: a filler, or what is left of a word, a stray letter or a full stop.
pub fn nothing_to_draw(text: &str) -> bool {
    crate::backchannel(text) || letters(text) <= 1
}

impl<P: Piece> Assembler<P> {
    /// Whether a turn is being held for its speaker to go on.
    pub fn holding(&self) -> bool {
        self.own.iter().any(|o| *o)
    }

    fn speaker(&self) -> Option<&Option<String>> {
        self.pending.iter().zip(&self.own).find(|(_, own)| **own).map(|(p, _)| &p.utterance().speaker)
    }

    fn text_len(&self) -> usize {
        self.pending.iter().zip(&self.own).filter(|(_, own)| **own).map(|(p, _)| p.utterance().text.chars().count()).sum()
    }

    /// Take a piece. What is ready to draw comes back, oldest first; a piece that may be continued is held.
    pub fn push(&mut self, piece: P) -> Vec<Ready<P>> {
        let mut out = vec![];
        let text = piece.utterance().text.clone();
        let speaker = piece.utterance().speaker.clone();
        if piece.urgent() {
            out.extend(self.flush());
            out.push(Ready { turn: Some(piece.utterance().clone()), pieces: vec![piece] });
            return out;
        }
        let empty = nothing_to_draw(&text);
        let holding = self.holding();
        let same = self.speaker() == Some(&speaker);
        if empty || (holding && !same && letters(&text) <= ASIDE_CHARS) {
            // An aside: kept with the turn it fell into, or passed on alone when there is none.
            if holding {
                self.pending.push(piece);
                self.own.push(false);
            } else {
                out.push(Ready { turn: None, pieces: vec![piece] });
            }
            return out;
        }
        let paused = match (self.last_end, piece.span()) {
            (Some(end), Some((start, _))) => start - end >= JOIN_GAP,
            _ => false,
        };
        if holding && (!same || paused) {
            out.extend(self.flush());
        }
        self.last_end = piece.span().map(|(_, end)| end);
        self.pending.push(piece);
        self.own.push(true);
        if self.text_len() >= MAX_CHARS {
            out.extend(self.flush());
        }
        out
    }

    /// Hand on what is held: the speaker has stopped, or there is nothing more to wait for.
    pub fn flush(&mut self) -> Option<Ready<P>> {
        if self.pending.is_empty() {
            return None;
        }
        let pieces = std::mem::take(&mut self.pending);
        let own = std::mem::take(&mut self.own);
        self.last_end = None;
        let words: Vec<&Utterance> = pieces.iter().zip(&own).filter(|(_, o)| **o).map(|(p, _)| p.utterance()).collect();
        let turn = words.first().map(|first| {
            let mut text = String::new();
            for w in &words {
                // Pieces of Chinese or Japanese run on; a Latin word needs its space.
                if text.chars().last().is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == ',') && w.text.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                    text.push(' ');
                }
                text.push_str(w.text.trim());
            }
            let mut pointing: Vec<String> = vec![];
            for id in words.iter().flat_map(|w| &w.pointing) {
                if !pointing.contains(id) {
                    pointing.push(id.clone());
                }
            }
            Utterance { speaker: first.speaker.clone(), text, pointing }
        });
        Some(Ready { turn, pieces })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Heard(Utterance, Option<(f64, f64)>, bool);

    impl Piece for Heard {
        fn utterance(&self) -> &Utterance {
            &self.0
        }
        fn span(&self) -> Option<(f64, f64)> {
            self.1
        }
        fn urgent(&self) -> bool {
            self.2
        }
    }

    fn said(who: &str, text: &str, from: f64, to: f64) -> Heard {
        Heard(Utterance { speaker: Some(who.into()), text: text.into(), pointing: vec![] }, Some((from, to)), false)
    }

    fn turns(pieces: Vec<Heard>) -> Vec<(Option<String>, usize)> {
        let mut a = Assembler::default();
        let mut out: Vec<Ready<Heard>> = vec![];
        for p in pieces {
            out.extend(a.push(p));
        }
        out.extend(a.flush());
        out.into_iter().map(|r| (r.turn.map(|t| t.text), r.pieces.len())).collect()
    }

    #[test]
    fn a_speakers_pieces_become_one_turn() {
        let got = turns(vec![
            said("C", "对，而且你现在设计的产品。", 0.0, 2.0),
            said("C", "主要推出的是它有照相的功能，功能的话是很。", 2.5, 6.0),
            said("C", "很很好，所以还是适合年轻的人群。", 6.4, 9.0),
            said("D", "我感觉可以根据年龄做一下区分。", 9.5, 12.0),
        ]);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0].0.as_deref(), Some("对，而且你现在设计的产品。主要推出的是它有照相的功能，功能的话是很。很很好，所以还是适合年轻的人群。"));
        assert_eq!(got[1].1, 1);
    }

    #[test]
    fn a_short_interruption_stays_inside_the_turn() {
        let got = turns(vec![
            said("D", "我感觉可以现在通过比较热门的直播带货，然后。", 0.0, 3.0),
            said("A", "是吧。", 3.1, 3.5),
            said("A", "对啊", 3.6, 3.9),
            said("D", "双十一这种活动我们都可以多参与一下。", 3.8, 6.0),
        ]);
        assert_eq!(got, vec![(Some("我感觉可以现在通过比较热门的直播带货，然后。双十一这种活动我们都可以多参与一下。".into()), 4)]);
    }

    #[test]
    fn a_pause_or_another_speaker_ends_a_turn() {
        let got = turns(vec![said("A", "先说注册。", 0.0, 1.0), said("A", "再说登录的流程怎么走。", 3.0, 5.0), said("B", "我来负责后端的这一块。", 5.2, 7.0)]);
        assert_eq!(got.iter().map(|t| t.0.as_deref().unwrap()).collect::<Vec<_>>(), ["先说注册。", "再说登录的流程怎么走。", "我来负责后端的这一块。"]);
    }

    #[test]
    fn nothing_to_draw_is_never_a_turn() {
        let got = turns(vec![said("C", "Y.", 0.0, 0.2), said("C", ".", 0.3, 0.4), said("A", "嗯。", 0.5, 0.7)]);
        assert_eq!(got, vec![(None, 1), (None, 1), (None, 1)]);
        assert!(nothing_to_draw("H.") && nothing_to_draw("是的。") && !nothing_to_draw("人群。"));
    }

    #[test]
    fn typed_or_pointing_lines_are_drawn_at_once() {
        let mut a = Assembler::default();
        assert!(a.push(said("A", "先说注册这一块的流程。", 0.0, 1.0)).is_empty(), "held for more");
        let typed = Heard(Utterance { speaker: Some("B".into()), text: "画个箭头".into(), pointing: vec!["n3".into()] }, None, true);
        let out = a.push(typed);
        assert_eq!(out.len(), 2, "what was held goes first, then the typed line");
        assert_eq!(out[1].turn.as_ref().unwrap().pointing, ["n3"]);
        assert!(!a.holding());
    }

    #[test]
    fn a_long_turn_is_handed_on_without_waiting() {
        let mut a = Assembler::default();
        let long = "这一段话说得很长，".repeat(10);
        assert!(a.push(said("A", &long, 0.0, 9.0)).is_empty());
        assert_eq!(a.push(said("A", &long, 9.1, 18.0)).len(), 1);
    }

    #[test]
    fn english_pieces_keep_their_spaces() {
        let got = turns(vec![said("A", "So the gateway calls", 0.0, 1.5), said("A", "the auth service first.", 1.7, 3.0)]);
        assert_eq!(got[0].0.as_deref(), Some("So the gateway calls the auth service first."));
    }
}
