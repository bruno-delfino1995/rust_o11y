use axum::response::Response;
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

pub fn layer() -> TraceLayer<
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
			otel.kind = %"server",
			otel.status_code = Empty,
			otel.status_message = Empty,

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
	fn on_failure(&mut self, failure: ServerErrorsFailureClass, _latency: Duration, span: &Span) {
		let message = format!("HTTP request failed: {}", failure);

		match failure {
			ServerErrorsFailureClass::StatusCode(status) => {
				if status.is_server_error() {
					span.record("otel.status_code", "ERROR");
					span.record("otel.status_message", message);
				}
			}
			ServerErrorsFailureClass::Error(_) => {
				span.record("otel.status_code", "ERROR");
				span.record("otel.status_message", message);
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

		opentelemetry::global::get_text_map_propagator(|propagator| propagator.extract(&extractor))
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
