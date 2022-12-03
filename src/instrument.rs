mod logs;
mod metrics;
mod traces;

use std::panic;
use tracing::{error, event, Level, Span};
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

pub fn init() {
	// let traces = traces::init();
	let logs = logs::init();
	metrics::init();

	let max_level = EnvFilter::try_from_env("LOG_LEVEL")
		.or_else(|_| EnvFilter::try_new("info"))
		.unwrap();

	let subscriber = Registry::default()
		.with(max_level)
		// .with(traces)
		.with(logs);

	tracing::subscriber::set_global_default(subscriber)
		.expect("Unable to register tracing subscriber");

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

		error!(message, panic = true, panic.file = file, panic.line = line)
	}));
}

pub fn stop() {
	// traces::stop();
}

pub mod axum {
	use axum::Router;
	use tower::ServiceBuilder;

	pub fn collectors(router: Router) -> Router {
		let metrics_layer = super::metrics::axum::layer();
		let trace_layer = super::traces::axum::layer();

		router.layer(
			ServiceBuilder::new()
				.layer(trace_layer)
				.layer(metrics_layer),
		)
	}

	pub fn reporters(router: Router) -> Router {
		super::metrics::axum::route(router)
	}
}
