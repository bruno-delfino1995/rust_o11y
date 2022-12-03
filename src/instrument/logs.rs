use std::collections::BTreeMap;
use std::borrow::Borrow;
use std::ops::Deref;
use std::ops::DerefMut;
use serde::Serialize;
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;
use tracing_subscriber::prelude::*;
use std::fmt;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use tracing_core::{Subscriber, Event};
use tracing_subscriber::fmt::{
	self as formatter,
	format::{self, FormatEvent, FormatFields},
	FmtContext,
	FormattedFields,
};


pub trait Sub: Subscriber + for<'span> LookupSpan<'span> {}
impl<T: Subscriber + for<'span> LookupSpan<'span>> Sub for T {}

pub fn init<S: Sub>() -> impl Layer<S> {
	CustomLayer
}

#[derive(Default, Clone)]
struct Store(BTreeMap<String, Value>);

impl Store {
	fn new() -> Store {
		Store(BTreeMap::new())
	}

	fn pull(&mut self, fields: Vec<&str>, from: &mut Store) {
		for field in fields.into_iter() {
			match from.remove_entry(field) {
				Some((key, value)) => self.0.insert(key, value),
				None => None,
			};
		}
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
	where S: serde::Serializer {
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

pub struct CustomLayer;
impl<S: Sub> Layer<S> for CustomLayer {
	fn on_new_span(
		&self,
		attrs: &tracing::span::Attributes<'_>,
		id: &tracing::span::Id,
		ctx: tracing_subscriber::layer::Context<'_, S>,
	) {
		let span = ctx.span(id).unwrap();
		let mut fields = Store::new();
		let mut visitor = JsonVisitor(&mut fields);
		attrs.record(&mut visitor);

		let storage = CustomFieldStorage(fields);
		let mut extensions = span.extensions_mut();
		extensions.insert(storage);
	}

	fn on_record(
		&self,
		id: &tracing::span::Id,
		values: &tracing::span::Record<'_>,
		ctx: tracing_subscriber::layer::Context<'_, S>,
	) {
		let span = ctx.span(id).unwrap();

		let mut extensions_mut = span.extensions_mut();
		let custom_field_storage: &mut CustomFieldStorage =
			extensions_mut.get_mut::<CustomFieldStorage>().unwrap();
		let json_data: &mut Store = &mut custom_field_storage.0;

		let mut visitor = JsonVisitor(json_data);
		values.record(&mut visitor);
	}


	fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
		let fields: Store = {
			let mut base = Store::new();
			let mut runtime = Store::new();

			let context_fields = ctx.event_scope(event).map(context_fields).unwrap_or_default();

			let mut event_fields = event_fields(event);
			base.pull(vec!["message"], &mut event_fields);

			let mut metadata_fields = metadata_fields(event);
			runtime.pull(vec!["file", "line", "target"], &mut metadata_fields);
			base.pull(vec!["level"], &mut metadata_fields);

			let mut additional_fields = additional_fields();
			base.pull(vec!["timestamp"], &mut additional_fields);
			runtime.pull(vec!["thread"], &mut additional_fields);

			base.push("context", context_fields);
			base.push("data", event_fields);
			base.push("runtime", runtime);

			base
		};

		let output = json!(fields);

		println!("{}", output.to_string());
	}
}

fn context_fields<S: Sub>(scope: tracing_subscriber::registry::Scope<S>) -> Store {
	let mut fields = Store::new();
	for span in scope.from_root() {
		let extensions = span.extensions();
		let storage = extensions.get::<CustomFieldStorage>().unwrap();
		let mut field_data: Store = storage.0.clone();

		fields.append(&mut field_data);
	}

	fields
}

fn event_fields(event: &tracing::Event<'_>) -> Store {
	let mut fields = Store::new();
	let mut visitor = JsonVisitor(&mut fields);
	event.record(&mut visitor);

	fields
}

fn metadata_fields(event: &tracing::Event<'_>) -> Store {
	let mut fields = Store::new();
	let meta = event.metadata();

	let data: Vec<(&str, Value)> = vec![
		("target", json!(meta.target())),
		("level", json!(format!("{:?}", meta.level()))),
		("line", json!(meta.line())),
		("file", json!(meta.file())),
	];

	for (key, value) in data {
		fields.insert(key.to_string(), value);
	}

	fields
}

fn additional_fields() -> Store {
	let mut fields = Store::new();

	let data: Vec<(&str, Value)> = vec![
		("thread", json!(format!("{:0>2?}", std::thread::current().id()))),
		("timestamp", json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)))
	];

	for (key, value) in data {
		fields.insert(key.to_string(), value);
	}

	fields
}

struct CustomFieldStorage(Store);

struct JsonVisitor<'a>(&'a mut Store);
impl<'a> tracing::field::Visit for JsonVisitor<'a> {
	fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
		self.0
			.insert(field.name().to_string(), json!(value));
	}

	fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
		self.0
			.insert(field.name().to_string(), json!(value));
	}

	fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
		self.0
			.insert(field.name().to_string(), json!(value));
	}

	fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
		self.0
			.insert(field.name().to_string(), json!(value));
	}

	fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
		self.0
			.insert(field.name().to_string(), json!(value));
	}

	fn record_error(
		&mut self,
		field: &tracing::field::Field,
		value: &(dyn std::error::Error + 'static),
	) {
		self.0.insert(
			field.name().to_string(),
			json!(value.to_string()),
		);
	}

	fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
		self.0.insert(
			field.name().to_string(),
			json!(format!("{:?}", value)),
		);
	}
}
