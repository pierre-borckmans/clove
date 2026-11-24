//! Progress events. Every long operation reports through a [`Reporter`] so the
//! CLI, TUI and desktop app can render the same steps their own way.

use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A timed step started.
    Start { step: String },
    /// A timed step finished.
    Done {
        step: String,
        #[serde(serialize_with = "ser_ms")]
        took: Duration,
    },
    /// A step was skipped, with the reason.
    Skip { step: String, reason: String },
    /// One line of output from a user command run inside a step.
    Output { step: String, line: String },
    /// Something the user should know about that does not stop the operation.
    Warn { message: String },
}

fn ser_ms<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64((d.as_secs_f64() * 1000.0).round())
}

pub trait Reporter: Send + Sync {
    fn event(&self, event: Event);
}

/// Drops every event. For callers that only want the result.
pub struct Silent;

impl Reporter for Silent {
    fn event(&self, _: Event) {}
}

impl<F: Fn(Event) + Send + Sync> Reporter for F {
    fn event(&self, event: Event) {
        self(event)
    }
}

/// Runs `f` as a named step, reporting start and duration.
pub fn timed<T>(
    r: &dyn Reporter,
    timings: &mut Vec<Timing>,
    step: &str,
    f: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    r.event(Event::Start { step: step.into() });
    let start = std::time::Instant::now();
    let out = f()?;
    let took = start.elapsed();
    r.event(Event::Done {
        step: step.into(),
        took,
    });
    timings.push(Timing {
        step: step.into(),
        ms: took.as_secs_f64() * 1000.0,
    });
    Ok(out)
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Timing {
    pub step: String,
    pub ms: f64,
}
