use opentelemetry::global;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use opentelemetry::sdk::trace as sdktrace;
use opentelemetry::sdk::Resource;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_semantic_conventions as semcov;
use tracing_subscriber::Layer;
use std::panic;
use tracing::error;
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, Registry, filter};

pub fn init() {
	let resource = Resource::new(vec![
		semcov::resource::SERVICE_NAME.string("gollum"),
		semcov::resource::SERVICE_VERSION.string("0.0.0"),
	]);

	let tracer = opentelemetry_otlp::new_pipeline()
		.tracing()
		.with_exporter(
			opentelemetry_otlp::new_exporter()
				.tonic()
				.with_endpoint("http://localhost:4317"),
		)
		.with_trace_config(sdktrace::config().with_resource(resource))
		.install_batch(opentelemetry::runtime::Tokio)
		.expect("Unable to create OTLP pipeline");

	let opentelemetry = tracing_opentelemetry::layer()
		.with_tracer(tracer)
		.with_exception_field_propagation(true)
	  .with_threads(true)
		.with_location(true)
		.with_tracked_inactivity(true)
		.with_filter(filter::filter_fn(|metadata| {
			metadata.is_span()
		}));

	let format = fmt::layer()
		.with_file(true)
		.with_target(true)
		.with_level(true)
		.with_line_number(true)
		.with_ansi(true)
		.with_thread_ids(true)
		.json();

	let max_level = EnvFilter::try_from_env("LOG_LEVEL")
		.or_else(|_| EnvFilter::try_new("info"))
		.unwrap();

	let subscriber = Registry::default()
		.with(max_level)
		.with(format)
		.with(opentelemetry);

	tracing::subscriber::set_global_default(subscriber)
		.expect("Unable to register tracing subscriber");

	global::set_text_map_propagator(TraceContextPropagator::new());

	panic::set_hook(Box::new(|info| {
		let message = match info.message() {
			Some(msg) => msg.to_string(),
			None => String::from("application crashed"),
		};

		let (file, line) = match info.location() {
			Some(location) => (Some(location.file()), Some(location.line())),
			None => (None, None),
		};

		error!(message, file = file, line = line)
	}))
}

pub fn stop() {
	opentelemetry::global::shutdown_tracer_provider();
}

pub mod axum {
	use axum::response::Response;
	use axum::Router;
	use http::Request;

	use std::time::Duration;
	use tower_http::{
		classify::{ServerErrorsAsFailures, ServerErrorsFailureClass, SharedClassifier},
		trace::{
			DefaultOnBodyChunk, DefaultOnEos, DefaultOnRequest, MakeSpan, OnFailure, OnResponse,
			TraceLayer,
		},
	};
	use tracing::{field::Empty, Span};

	pub fn setup(router: Router) -> Router {
		router.layer(layer())
	}

	fn layer() -> TraceLayer<
		SharedClassifier<ServerErrorsAsFailures>,
		OtelMakeSpan,
		DefaultOnRequest,
		OtelOnResponse,
		DefaultOnBodyChunk,
		DefaultOnEos,
		OtelOnFailure,
	> {
		TraceLayer::new_for_http()
			.make_span_with(OtelMakeSpan)
			.on_response(OtelOnResponse)
			.on_failure(OtelOnFailure)
	}

	#[derive(Clone, Copy, Debug)]
	pub struct OtelMakeSpan;

	impl<B> MakeSpan<B> for OtelMakeSpan {
		fn make_span(&mut self, req: &Request<B>) -> Span {
			let http = request::info(req);
			let name = format!("{} {}", http.method, http.route);

			let span = tracing::info_span!(
				"HTTP Request",
				otel.name = %name,
				otel.kind = %"server", // opentelemetry::trace::SpanKind::Server
				otel.status_code = Empty,
				http.client_ip = %http.client_ip,
				http.flavor = %http.flavor,
				http.host = %http.host,
				http.method = %http.method,
				http.route = %http.route,
				http.scheme = %http.scheme,
				http.status_code = Empty,
				http.target = %http.target,
				http.user_agent = %http.user_agent,
			);

			let remote_context = context::create(context::extract(req.headers()));
			tracing_opentelemetry::OpenTelemetrySpanExt::set_parent(&span, remote_context);

			span
		}
	}

	#[derive(Clone, Copy, Debug)]
	pub struct OtelOnResponse;

	impl<B> OnResponse<B> for OtelOnResponse {
		fn on_response(self, response: &Response<B>, _latency: Duration, span: &Span) {
			let status = response.status().as_u16().to_string();
			span.record("http.status_code", &tracing::field::display(status));

			// assume there is no error, if there is `OtelOnFailure` will be called and override this
			span.record("otel.status_code", "OK");
		}
	}

	#[derive(Clone, Copy, Debug)]
	pub struct OtelOnFailure;

	impl OnFailure<ServerErrorsFailureClass> for OtelOnFailure {
		fn on_failure(
			&mut self,
			failure: ServerErrorsFailureClass,
			_latency: Duration,
			span: &Span,
		) {
			match failure {
				ServerErrorsFailureClass::StatusCode(status) => {
					if status.is_server_error() {
						span.record("otel.status_code", "ERROR");
					}
				}
				ServerErrorsFailureClass::Error(_) => {
					span.record("otel.status_code", "ERROR");
				}
			}
		}
	}

	mod context {
		use opentelemetry::sdk::trace::{IdGenerator, RandomIdGenerator};
		use opentelemetry::trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceState};

		struct HeaderExtractor<'a>(&'a http::HeaderMap);
		impl<'a> opentelemetry::propagation::Extractor for HeaderExtractor<'a> {
			fn get(&self, key: &str) -> Option<&str> {
				self.0.get(key).and_then(|value| value.to_str().ok())
			}

			fn keys(&self) -> Vec<&str> {
				self.0.keys().map(|value| value.as_str()).collect()
			}
		}

		// If remote request has no span data the propagator defaults to an unsampled context
		pub fn extract(headers: &http::HeaderMap) -> opentelemetry::Context {
			let extractor = HeaderExtractor(headers);

			opentelemetry::global::get_text_map_propagator(|propagator| {
				propagator.extract(&extractor)
			})
		}

		// Create a valid sampled context with a trace_id (if not set) before call to
		// `tracing_opentelemetry::OpenTelemetrySpanExt::set_parent` else trace_id is defined too late
		// and the `info_span` log `trace_id: ""` Use the default global tracer (named "") to start the
		// trace
		pub fn create(remote_context: opentelemetry::Context) -> opentelemetry::Context {
			if !remote_context.span().span_context().is_valid() {
				let trace_id = RandomIdGenerator::default().new_trace_id();
				let span_context = SpanContext::new(
					trace_id,
					SpanId::INVALID,
					TraceFlags::SAMPLED,
					false,
					TraceState::default(),
				);

				remote_context.with_remote_span_context(span_context)
			} else {
				remote_context
			}
		}
	}

	mod request {
		use axum::extract::{ConnectInfo, MatchedPath, OriginalUri};
		use http::Request;
		use http::{header, uri::Scheme, HeaderMap, Method, Version};

		pub struct Info<'a> {
			pub client_ip: &'a str,
			pub flavor: &'a str,
			pub host: &'a str,
			pub method: &'a str,
			pub route: &'a str,
			pub scheme: &'a str,
			pub target: &'a str,
			pub user_agent: &'a str,
		}

		pub fn info<B>(req: &Request<B>) -> Info<'_> {
			let user_agent = req
				.headers()
				.get(header::USER_AGENT)
				.map_or("", |h| h.to_str().unwrap_or(""));

			let host = req
				.headers()
				.get(header::HOST)
				.map_or("", |h| h.to_str().unwrap_or(""));

			let scheme = req.uri().scheme().map_or_else(|| "HTTP", http_scheme);

			let route = req
				.extensions()
				.get::<MatchedPath>()
				.map_or("", |mp| mp.as_str());

			let uri = if let Some(uri) = req.extensions().get::<OriginalUri>() {
				&uri.0
			} else {
				req.uri()
			};

			let target = uri
				.path_and_query()
				.map(|path_and_query| path_and_query.as_str())
				.unwrap_or_else(|| uri.path());

			let client_ip = parse_x_forwarded_for(req.headers())
				.or_else(|| {
					req.extensions()
						.get::<ConnectInfo<String>>()
						.map(|ConnectInfo(client_ip)| client_ip.as_str())
				})
				.unwrap_or_default();

			let method = http_method(req.method());

			let flavor = http_flavor(&req.version());

			Info {
				client_ip,
				flavor,
				host,
				method,
				route,
				scheme,
				target,
				user_agent,
			}
		}

		fn parse_x_forwarded_for(headers: &HeaderMap) -> Option<&str> {
			let value = headers.get("x-forwarded-for")?;
			let value = value.to_str().ok()?;
			let mut ips = value.split(',');

			Some(ips.next()?.trim())
		}

		fn http_method(method: &Method) -> &str {
			match *method {
				Method::CONNECT => "CONNECT",
				Method::DELETE => "DELETE",
				Method::GET => "GET",
				Method::HEAD => "HEAD",
				Method::OPTIONS => "OPTIONS",
				Method::PATCH => "PATCH",
				Method::POST => "POST",
				Method::PUT => "PUT",
				Method::TRACE => "TRACE",
				_ => "",
			}
		}

		fn http_flavor(version: &Version) -> &'static str {
			match *version {
				Version::HTTP_09 => "0.9",
				Version::HTTP_10 => "1.0",
				Version::HTTP_11 => "1.1",
				Version::HTTP_2 => "2.0",
				Version::HTTP_3 => "3.0",
				_ => "",
			}
		}

		fn http_scheme(scheme: &Scheme) -> &str {
			if scheme == &Scheme::HTTP {
				"http"
			} else if scheme == &Scheme::HTTPS {
				"https"
			} else {
				scheme.as_ref()
			}
		}
	}
}
