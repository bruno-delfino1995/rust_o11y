#![feature(panic_info_message, thread_id_value)]

pub mod http;
mod level;
mod logs;
mod metrics;
mod traces;

use std::panic;
use tracing::{error, Span};
use tracing_core::Subscriber;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub trait Sub: Subscriber + for<'span> LookupSpan<'span> {}
impl<T: Subscriber + for<'span> LookupSpan<'span>> Sub for T {}

/// Guard used to control cleanup of instrumentation configs
///
/// There's an empty unit field to prevent outsiders from creating it manually
pub struct Instrument(());

pub fn init() -> Instrument {
	tracing_subscriber::registry()
		.with(level::init())
		.with(traces::init())
		.with(logs::init())
		.try_init()
		.expect("Unable to register tracing subscriber");

	metrics::init();

	panic::set_hook(Box::new(|info| {
		let message = match info.message() {
			Some(msg) => msg.to_string(),
			None => String::from("application crashed"),
		};

		let (file, line) = match info.location() {
			Some(location) => (Some(location.file()), Some(location.line())),
			None => (None, None),
		};

		let span = Span::current();
		span.record("otel.status_code", "ERROR");
		span.record("otel.status_message", "panic");

		error!(message, panic.file = file, panic.line = line)
	}));

	Instrument(())
}

impl Drop for Instrument {
	fn drop(&mut self) {
		traces::stop();
	}
}
