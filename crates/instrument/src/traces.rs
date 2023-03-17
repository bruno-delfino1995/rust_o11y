use super::Sub;

use opentelemetry::global;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use opentelemetry::sdk::trace as sdktrace;
use opentelemetry::sdk::Resource;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_semantic_conventions as semcov;
use tracing_subscriber::filter;

use tracing_subscriber::Layer;

pub struct Options<'a> {
	pub service: &'a str,
	pub version: &'a str,
	pub exporter: &'a str,
}

pub fn init<S: Sub>(opts: Options) -> impl Layer<S> {
	global::set_text_map_propagator(TraceContextPropagator::new());

	let resource = Resource::new(vec![
		semcov::resource::SERVICE_NAME.string(opts.service.to_string()),
		semcov::resource::SERVICE_VERSION.string(opts.version.to_string()),
	]);

	let tracer = opentelemetry_otlp::new_pipeline()
		.tracing()
		.with_exporter(
			opentelemetry_otlp::new_exporter()
				.tonic()
				.with_endpoint(opts.exporter),
		)
		.with_trace_config(sdktrace::config().with_resource(resource))
		.install_batch(opentelemetry::runtime::Tokio)
		.expect("Unable to create OTLP pipeline");

	tracing_opentelemetry::layer()
		.with_tracer(tracer)
		.with_exception_field_propagation(true)
		.with_threads(true)
		.with_location(true)
		.with_tracked_inactivity(true)
		.with_filter(filter::filter_fn(|metadata| metadata.is_span()))
}

pub fn stop() {
	opentelemetry::global::shutdown_tracer_provider();
}
