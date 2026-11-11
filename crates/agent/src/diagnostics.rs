//! Bounded stage timings without meeting content, provider URLs, model names or errors.
use serde_json::{json, Value};
use std::collections::VecDeque;

pub const RECENT: usize = 128;

pub fn stats(values: impl Iterator<Item = u128>) -> Value {
    let mut values: Vec<_> = values.collect();
    if values.is_empty() { return Value::Null; }
    values.sort_unstable();
    json!({ "n": values.len(), "p50": values[(values.len() - 1) / 2],
        "p95": values[((values.len() - 1) as f64 * 0.95).ceil() as usize], "max": values.last() })
}

pub(crate) struct Batch {
    pub backend: &'static str,
    pub outcome: &'static str,
    pub lines: usize,
    pub queue_ms: u128,
    pub provider_ms: u128,
    pub generation_ms: Option<u128>,
}

#[derive(Default)]
pub(crate) struct Report {
    seen: u64,
    pub ignored_lines: usize,
    recent: VecDeque<(u64, Batch)>,
}

impl Report {
    pub fn push(&mut self, batch: Batch) -> u64 {
        self.seen += 1;
        if self.recent.len() == RECENT { self.recent.pop_front(); }
        self.recent.push_back((self.seen, batch));
        self.seen
    }

    pub fn json(&self) -> Value {
        let mut groups = vec![];
        for backend in ["local", "remote", "unavailable", "scripted"] {
            for outcome in ["applied", "failed", "superseded"] {
                let batches: Vec<_> = self.recent.iter().map(|(_, b)| b)
                    .filter(|b| b.backend == backend && b.outcome == outcome).collect();
                if batches.is_empty() { continue; }
                groups.push(json!({ "backend": backend, "outcome": outcome, "batches": batches.len(),
                    "lines": batches.iter().map(|b| b.lines).sum::<usize>(),
                    "queue_ms": stats(batches.iter().map(|b| b.queue_ms)),
                    "provider_ms": stats(batches.iter().map(|b| b.provider_ms)),
                    "generation_ms": stats(batches.iter().filter_map(|b| b.generation_ms)) }));
            }
        }
        json!({ "batches_seen": self.seen, "ignored_queued_lines": self.ignored_lines,
            "recent_limit": RECENT, "groups": groups,
            "recent": self.recent.iter().map(|(id, b)| json!({ "id": id, "backend": b.backend,
                "outcome": b.outcome, "lines": b.lines, "queue_ms": b.queue_ms,
                "provider_ms": b.provider_ms, "generation_ms": b.generation_ms })).collect::<Vec<_>>(),
            "timing_basis": "monotonic wall time; queue starts at submission of the oldest line in a batch and includes initial model readiness; provider is lookup/startup; generation includes prompt preparation, retries and reply validation; not acoustic or screen latency; request order does not establish cold/warm cache state"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reports_keep_failures_separate_and_unmeasured_stages_null() {
        let mut report = Report::default();
        for i in 0..=RECENT {
            report.push(Batch { backend: "unavailable", outcome: "failed", lines: 1,
                queue_ms: i as u128, provider_ms: 2, generation_ms: None });
        }
        let value = report.json();
        assert_eq!(value["batches_seen"], RECENT + 1);
        assert_eq!(value["recent"].as_array().unwrap().len(), RECENT);
        assert_eq!(value["recent"][0]["id"], 2);
        assert_eq!(value["groups"][0]["queue_ms"]["n"], RECENT);
        assert_eq!(value["groups"][0]["queue_ms"]["max"], RECENT);
        assert!(value["groups"][0]["generation_ms"].is_null());
        report.push(Batch { backend: "local", outcome: "superseded", lines: 2,
            queue_ms: 0, provider_ms: 0, generation_ms: Some(7) });
        let value = report.json();
        assert_eq!(value["groups"][0]["outcome"], "superseded");
        assert_eq!(value["groups"][0]["generation_ms"]["p50"], 7);
        assert_eq!(value["groups"][1]["batches"], RECENT - 1);
        assert!(stats(std::iter::empty()).is_null());
        assert_eq!(stats([9, 1, 5].into_iter()), json!({ "n": 3, "p50": 5, "p95": 9, "max": 9 }));
    }
}
