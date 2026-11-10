//! The sample meeting the page tells: a short release review, with when each line was said.

use crate::util::two;

pub const MEETING: f64 = 12.0 * 60.0; // seconds

pub struct Said {
    pub t: f64,
    pub who: &'static str,
    pub say: &'static str,
}

pub const LINES: [Said; 6] = [
    Said {
        t: 30.0,
        who: "A",
        say: "Let’s list what this release needs.",
    },
    Said {
        t: 108.0,
        who: "A",
        say: "Run the regression tests, ship on Friday.",
    },
    Said {
        t: 192.0,
        who: "B",
        say: "Canary at 10%, then 50% if it holds.",
    },
    Said {
        t: 336.0,
        who: "C",
        say: "Not Friday. Make it next Tuesday.",
    },
    Said {
        t: 474.0,
        who: "B",
        say: "Star this one.",
    },
    Said {
        t: 624.0,
        who: "C",
        say: "Who owns the rollback plan?",
    },
];

/// A moment of the meeting as a clock time; it started at 14:02.
pub fn clock_at(sec: f64) -> String {
    format!("14:{}:{}", two(2.0 + (sec / 60.0).floor()), two(sec % 60.0))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_starts_at_two_past_two() {
        assert_eq!(super::clock_at(0.0), "14:02:00");
        assert_eq!(super::clock_at(125.5), "14:04:05");
    }
}
