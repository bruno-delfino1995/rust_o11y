mod store;

use self::store::{PortBy, Store};
use super::Sub;
use chrono::DateTime;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};
use std::thread::ThreadId;
use tracing::Metadata;
use tracing_core::Event;
use tracing_subscriber::registry::Scope;
use tracing_subscriber::Layer;

pub fn init<S: Sub>() -> impl Layer<S> {
	LogLayer
}

struct LogLayer;
impl<S: Sub> Layer<S> for LogLayer {
	fn on_new_span(
		&self,
		attrs: &tracing::span::Attributes<'_>,
		id: &tracing::span::Id,
		ctx: tracing_subscriber::layer::Context<'_, S>,
	) {
		let span = ctx.span(id).unwrap();

		let mut store = Store::new();
		attrs.record(&mut store);

		let mut extensions = span.extensions_mut();
		extensions.insert(store);
	}

	fn on_record(
		&self,
		id: &tracing::span::Id,
		values: &tracing::span::Record<'_>,
		ctx: tracing_subscriber::layer::Context<'_, S>,
	) {
		let span = ctx.span(id).unwrap();

		let mut extensions = span.extensions_mut();
		let store: &mut Store = extensions.get_mut::<Store>().unwrap();

		values.record(store);
	}

	fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
		let fields: Store = {
			let context: Store = ctx.event_scope(event).map(Scope::into).unwrap_or_default();
			let mut metadata: Store = event.metadata().into();
			let mut event: Store = event.into();
			let mut live: Store = (&Live::new()).into();

			let mut runtime = Store::new();
			runtime.port(&mut live, vec!["thread"]);
			runtime
				.port_by(&mut event, by_prefix("panic.", vec!["line", "file"]))
				.or_else(|runtime| {
					runtime.port_by(
						&mut event,
						by_prefix("log.", vec!["target", "line", "file"]),
					)
				})
				.or_else(|runtime| runtime.port(&mut metadata, vec!["target", "line", "file"]));

			let mut root = Store::new();
			root.port(&mut event, vec!["message"]);
			root.port(&mut metadata, vec!["level"]);
			root.port(&mut live, vec!["timestamp"]);

			root.push("context", context);
			root.push("data", event);
			root.push("runtime", runtime);

			root
		};

		let output = json!(fields);

		println!("{}", output);
	}
}

fn by_prefix<'a>(prefix: &'a str, allowed: Vec<&'a str>) -> PortBy<'a> {
	Box::new(move |key| {
		if !key.starts_with(prefix) {
			return None;
		}

		let key = key.strip_prefix(prefix).unwrap_or_default();

		if allowed.contains(&key) {
			Some(key.to_string())
		} else {
			None
		}
	})
}

struct Live {
	thread: ThreadId,
	now: DateTime<Utc>,
}

impl Live {
	fn new() -> Live {
		Live {
			thread: std::thread::current().id(),
			now: Utc::now(),
		}
	}
}

impl<S: Sub> From<Scope<'_, S>> for Store {
	fn from(value: Scope<'_, S>) -> Self {
		let mut fields = Store::new();
		for span in value.from_root() {
			let extensions = span.extensions();
			let mut store = extensions.get::<Store>().unwrap().clone();

			fields.port_all(&mut store);
		}

		fields
	}
}

impl From<&Metadata<'_>> for Store {
	fn from(value: &Metadata<'_>) -> Self {
		let mut fields = Store::new();

		let data: Vec<(&str, Value)> = vec![
			("target", json!(value.target())),
			("level", json!(value.level().to_string().to_lowercase())),
			("line", json!(value.line())),
			("file", json!(value.file())),
		];

		for (key, value) in data {
			fields.insert(key.to_string(), value);
		}

		fields
	}
}

impl From<&Event<'_>> for Store {
	fn from(value: &Event<'_>) -> Self {
		let mut store = Store::new();
		value.record(&mut store);

		store
	}
}

impl From<&Live> for Store {
	fn from(value: &Live) -> Self {
		let mut fields = Store::new();

		let data: Vec<(&str, Value)> = vec![
			("thread", json!(value.thread.as_u64())),
			(
				"timestamp",
				json!(value.now.to_rfc3339_opts(SecondsFormat::Millis, true)),
			),
		];

		for (key, value) in data {
			fields.insert(key.to_string(), value);
		}

		fields
	}
}
