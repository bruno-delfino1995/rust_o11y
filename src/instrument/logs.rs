use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::borrow::Borrow;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;
use tracing::field::Visit;
use tracing::Metadata;
use tracing_core::{Event, Subscriber};
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::fmt::{
	self as formatter,
	format::{self, FormatEvent, FormatFields},
	FmtContext, FormattedFields,
};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::registry::Scope;
use tracing_subscriber::Layer;

pub trait Sub: Subscriber + for<'span> LookupSpan<'span> {}
impl<T: Subscriber + for<'span> LookupSpan<'span>> Sub for T {}

type PortBy<'a> = Box<dyn Fn(&str) -> Option<String> + 'a>;

struct Live;

#[derive(Default, Clone)]
struct Store(BTreeMap<String, Value>);

impl Store {
	fn new() -> Store {
		Store(BTreeMap::new())
	}

	fn port(&mut self, from: &mut Store, keys: Vec<&str>) -> Result<(), &mut Self> {
		let mut ported = Err(());

		for k in keys {
			match from.remove_entry(k) {
				Some((k, v)) => {
					self.0.insert(k, v);
					ported = Ok(());
				}
				None => continue,
			};
		}

		ported.map_err(|_| self)
	}

	fn port_by(
		&mut self,
		from: &mut Store,
		predicate: PortBy,
	) -> Result<(), &mut Self> {
		let mut ported = Err(());

		from.retain(|k, v| match predicate(k) {
			None => true,
			Some(nk) => {
				self.insert(nk, v.clone());
				ported = Ok(());

				false
			}
		});

		ported.map_err(|_| self)
	}

	fn port_all(&mut self, from: &mut Store) {
		self.0.append(&mut from.0);
	}

	fn push(&mut self, field: &str, from: Store) {
		if from.is_empty() {
			return;
		}

		self.0.insert(field.to_string(), json!(from));
	}
}

impl Serialize for Store {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		self.0.serialize(serializer)
	}
}

impl Deref for Store {
	type Target = BTreeMap<String, Value>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl DerefMut for Store {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

pub fn init<S: Sub>() -> impl Layer<S> {
	CustomLayer
}

pub struct CustomLayer;
impl<S: Sub> Layer<S> for CustomLayer {
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
		let store: &mut Store =
			extensions.get_mut::<Store>().unwrap();

		values.record(store);
	}

	fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
		let fields: Store = {
			let mut context: Store = ctx.event_scope(event).map(Scope::into).unwrap_or_default();
			let mut metadata: Store = event.metadata().into();
			let mut event: Store = event.into();
			let mut live: Store = (&Live).into();

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

fn by_prefix<'a>(
	prefix: &'a str,
	allowed: Vec<&'a str>,
) -> PortBy<'a> {
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
			("level", json!(format!("{:?}", value.level()))),
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
	fn from(_value: &Live) -> Self {
		let mut fields = Store::new();

		let data: Vec<(&str, Value)> = vec![
			(
				"thread",
				json!(format!("{:0>2?}", std::thread::current().id())),
			),
			(
				"timestamp",
				json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
			),
		];

		for (key, value) in data {
			fields.insert(key.to_string(), value);
		}

		fields
	}
}

impl Visit for Store {
	fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
		self.insert(field.name().to_string(), json!(value));
	}

	fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
		self.insert(field.name().to_string(), json!(value));
	}

	fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
		self.insert(field.name().to_string(), json!(value));
	}

	fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
		self.insert(field.name().to_string(), json!(value));
	}

	fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
		self.insert(field.name().to_string(), json!(value));
	}

	fn record_error(
		&mut self,
		field: &tracing::field::Field,
		value: &(dyn std::error::Error + 'static),
	) {
		self.insert(field.name().to_string(), json!(value.to_string()));
	}

	fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
		self.insert(field.name().to_string(), json!(format!("{:?}", value)));
	}
}
