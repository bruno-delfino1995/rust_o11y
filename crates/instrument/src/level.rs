use super::Sub;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

pub fn init<S: Sub>() -> impl Layer<S> {
	EnvFilter::try_from_env("LOG_LEVEL")
		.or_else(|_| EnvFilter::try_new("info"))
		.unwrap()
}
