use std::panic;
use tracing::{error, Span};
use tracing_core::Subscriber;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn init<S: Sub>() -> impl Layer<S> {
	EnvFilter::try_from_env("LOG_LEVEL")
		.or_else(|_| EnvFilter::try_new("info"))
		.unwrap()
}
