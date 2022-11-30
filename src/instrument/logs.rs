use tracing_subscriber::fmt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

pub fn init<S: tracing::Subscriber + for<'span> LookupSpan<'span>>() -> impl Layer<S> {
	fmt::layer()
		.with_file(true)
		.with_target(true)
		.with_level(true)
		.with_line_number(true)
		.with_ansi(true)
		.with_thread_ids(true)
		.json()
}
