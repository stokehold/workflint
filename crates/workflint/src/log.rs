//! The log of `-verbose` and `-debug` on stderr (ADR 0013).
//!
//! The driver logs through `tracing`, with each file's events in a [`FILE_SPAN`] span. This
//! layer holds the lines of a file until its span closes and then writes them in one piece, so
//! the lines of files linted in parallel never interleave. A line outside a file's span is
//! written at once. Each line has the reference's prefix: `verbose: ` for `INFO` events, which
//! `-verbose` shows, and `[Linter] ` for `DEBUG` events, which `-debug` adds.

use std::fmt::{self, Write as _};
use std::io::{self, Write};

use tracing::field::{Field, Visit};
use tracing::span::Id;
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use workflint_linter::FILE_SPAN;

/// Sends the log events of workflint up to `max` to stderr for the rest of the run.
pub(crate) fn init(max: Level) {
    let subscriber = tracing_subscriber::registry().with(FileLog { max });
    // Only `main` sets the subscriber, once.
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// The layer that writes the log.
struct FileLog {
    max: Level,
}

/// The lines of one file's span that are not written yet.
#[derive(Default)]
struct Lines(String);

impl<S> Layer<S> for FileLog
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        *metadata.level() <= self.max && metadata.target().starts_with("workflint")
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut line = String::from(if *event.metadata().level() <= Level::INFO {
            "verbose: "
        } else {
            "[Linter] "
        });
        event.record(&mut Message(&mut line));
        line.push('\n');
        let file = ctx
            .event_scope(event)
            .and_then(|mut scope| scope.find(|span| span.name() == FILE_SPAN));
        match file {
            Some(span) => {
                let mut extensions = span.extensions_mut();
                match extensions.get_mut::<Lines>() {
                    Some(lines) => lines.0.push_str(&line),
                    None => extensions.insert(Lines(line)),
                }
            }
            None => write(&line),
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let lines = ctx
            .span(&id)
            .and_then(|span| span.extensions_mut().remove::<Lines>());
        if let Some(Lines(lines)) = lines {
            write(&lines);
        }
    }
}

/// Writes `text` to stderr in one call, so that no other line comes between its lines.
fn write(text: &str) {
    // Nothing is left to report a failed write of the log to.
    let _ = io::stderr().lock().write_all(text.as_bytes());
}

/// Reads the `message` field of an event.
struct Message<'a>(&'a mut String);

impl Visit for Message<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        }
    }
}
