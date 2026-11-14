//! The measures a part gives the monitoring that is there, in the text format that
//! Prometheus and its kin read.
//!
//! ```text
//! # HELP away_requests_total Requests answered, by listener and by the kind of answer.
//! # TYPE away_requests_total counter
//! away_requests_total{part="server",listener="public",answer="2xx"} 12
//! ```
//!
//! A measure is a counter, which only grows, or a gauge, which is set. Each has a name,
//! a sentence of help, and one series for every set of labels it was asked for with.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A number that only grows.
#[derive(Clone)]
pub struct Counter(Arc<AtomicU64>);

impl Counter {
    /// One more.
    pub fn add_one(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// What it counts now.
    pub fn value(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// A number that is set to what is true now.
#[derive(Clone)]
pub struct Gauge(Arc<AtomicU64>);

impl Gauge {
    /// It is this now.
    pub fn set(&self, value: u64) {
        self.0.store(value, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Counter,
    Gauge,
}

/// One set of labels of a measure, with its number.
struct Series {
    labels: Vec<(&'static str, String)>,
    value: Arc<AtomicU64>,
}

struct Measure {
    name: &'static str,
    help: &'static str,
    kind: Kind,
    series: Vec<Series>,
}

/// The measures of one part. Every series carries the part's name as its first label.
#[derive(Clone)]
pub struct Measures {
    part: &'static str,
    all: Arc<Mutex<Vec<Measure>>>,
}

impl Measures {
    /// The measures of a part, none yet.
    pub fn of(part: &'static str) -> Self {
        Measures { part, all: Arc::new(Mutex::new(Vec::new())) }
    }

    /// A counter with these labels. Asking again with the same name and labels gives the
    /// same counter.
    pub fn counter(&self, name: &'static str, help: &'static str, labels: &[(&'static str, &str)]) -> Counter {
        Counter(self.series(name, help, Kind::Counter, labels))
    }

    /// A gauge with these labels. Asking again with the same name and labels gives the
    /// same gauge.
    pub fn gauge(&self, name: &'static str, help: &'static str, labels: &[(&'static str, &str)]) -> Gauge {
        Gauge(self.series(name, help, Kind::Gauge, labels))
    }

    fn series(&self, name: &'static str, help: &'static str, kind: Kind, labels: &[(&'static str, &str)]) -> Arc<AtomicU64> {
        let mut wanted = vec![("part", self.part.to_string())];
        wanted.extend(labels.iter().map(|(label, value)| (*label, (*value).to_string())));
        let mut all = self.all.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let at = match all.iter().position(|measure| measure.name == name) {
            Some(at) => at,
            None => {
                all.push(Measure { name, help, kind, series: Vec::new() });
                all.len() - 1
            }
        };
        let measure = &mut all[at];
        if let Some(series) = measure.series.iter().find(|series| series.labels == wanted) {
            return series.value.clone();
        }
        let value = Arc::new(AtomicU64::new(0));
        measure.series.push(Series { labels: wanted, value: value.clone() });
        value
    }

    /// Every measure, as the text a monitoring reads.
    pub fn text(&self) -> String {
        let all = self.all.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut out = String::new();
        for measure in all.iter() {
            let kind = if measure.kind == Kind::Counter { "counter" } else { "gauge" };
            let _ = writeln!(out, "# HELP {} {}", measure.name, measure.help);
            let _ = writeln!(out, "# TYPE {} {kind}", measure.name);
            for Series { labels, value } in &measure.series {
                out.push_str(measure.name);
                out.push('{');
                for (at, (label, text)) in labels.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    out.push_str(label);
                    out.push_str("=\"");
                    for character in text.chars() {
                        match character {
                            '\\' => out.push_str("\\\\"),
                            '"' => out.push_str("\\\""),
                            '\n' => out.push_str("\\n"),
                            other => out.push(other),
                        }
                    }
                    out.push('"');
                }
                let _ = writeln!(out, "}} {}", value.load(Ordering::Relaxed));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_counter_is_written_with_its_help_its_kind_and_its_labels() {
        let measures = Measures::of("server");
        let answered = measures.counter("away_requests_total", "Requests answered.", &[("listener", "public"), ("answer", "2xx")]);
        answered.add_one();
        answered.add_one();
        assert_eq!(
            measures.text(),
            "# HELP away_requests_total Requests answered.\n\
             # TYPE away_requests_total counter\n\
             away_requests_total{part=\"server\",listener=\"public\",answer=\"2xx\"} 2\n"
        );
    }

    #[test]
    fn the_same_name_and_labels_are_the_same_series() {
        let measures = Measures::of("gateway");
        measures.counter("away_requests_total", "Requests answered.", &[("answer", "2xx")]).add_one();
        measures.counter("away_requests_total", "Requests answered.", &[("answer", "2xx")]).add_one();
        measures.counter("away_requests_total", "Requests answered.", &[("answer", "4xx")]).add_one();
        let text = measures.text();
        assert_eq!(text.matches("# TYPE away_requests_total counter").count(), 1);
        assert!(text.contains("away_requests_total{part=\"gateway\",answer=\"2xx\"} 2\n"));
        assert!(text.contains("away_requests_total{part=\"gateway\",answer=\"4xx\"} 1\n"));
    }

    #[test]
    fn a_gauge_says_what_is_true_now_and_a_label_is_escaped() {
        let measures = Measures::of("issuer");
        let ready = measures.gauge("away_ready", "Whether the part is ready.", &[("why", "a \"quoted\" \\ word")]);
        ready.set(1);
        ready.set(0);
        assert!(measures.text().contains("# TYPE away_ready gauge\naway_ready{part=\"issuer\",why=\"a \\\"quoted\\\" \\\\ word\"} 0\n"));
    }
}
