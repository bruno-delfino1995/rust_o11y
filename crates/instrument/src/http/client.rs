use reqwest_middleware::{ClientBuilder, Extension};
use reqwest_tracing::{OtelName, TracingMiddleware};

pub fn decorate(builder: ClientBuilder) -> ClientBuilder {
	builder
		.with_init(Extension(OtelName("localhost".into())))
		.with(TracingMiddleware::default())
}
