use std::collections::BTreeMap;
use std::borrow::Borrow;
use tracing_subscriber::fmt::format::JsonFields;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;
use tracing_subscriber::prelude::*;
use std::fmt;
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};
use tracing_core::{Subscriber, Event};
use tracing_subscriber::fmt::{
	self as formatter,
	format::{self, FormatEvent, FormatFields},
	FmtContext,
	FormattedFields,
};

type Store = BTreeMap<String, Value>;

pub trait Sub: Subscriber + for<'span> LookupSpan<'span> {}
impl<T: Subscriber + for<'span> LookupSpan<'span>> Sub for T {}

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
		let mut fields: Store = {
			let mut context_fields = ctx.event_scope(event).map(context_fields).unwrap_or_default();
			let mut event_fields = event_fields(event);
			let mut metadata_fields = metadata_fields(event);
			let mut additional_fields = additional_fields();

			context_fields.append(&mut event_fields);
			context_fields.append(&mut metadata_fields);
			context_fields.append(&mut additional_fields);

			context_fields
		};

		let output = format(fields);

		// println!("{}", serde_json::to_string_pretty(&output).unwrap());
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
		("name", json!(meta.name())),
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

fn format(mut fields: Store) -> Value {
	let mut base = Store::new();
	let mut runtime = Store::new();

	pull(vec!["message", "timestamp", "level"], &mut fields, &mut base);
	pull(vec!["thread", "file", "line", "target"], &mut fields, &mut runtime);

	base.insert("runtime".to_string(), json!(runtime));
	base.insert("context".to_string(), json!(fields));

	json!(base)
}

fn pull(fields: Vec<&str>, from: &mut Store, into: &mut Store) {
	for field in fields.into_iter() {
		match from.remove_entry(field) {
			Some((key, value)) => into.insert(key, value),
			None => None,
		};
	}
}

fn set_path(
	mut obj: &mut Value,
	path: impl IntoIterator<Item=impl Borrow<str>>,
	value: Value,
) {
	let mut path = path.into_iter();

	// Start with nothing in "a" and the first item in "b".
	let mut a;
	let mut b = path.next();

	loop {
		// Shift "b" down into "a" and put the next item into "b".
		a = b;
		b = path.next();

		// Move "a" but borrow "b" because we will use it on the next iteration.
		match (a, &b) {
			(Some(key), Some(_)) => {
				// This level is an object, rebind deeper.
				obj = &mut obj[key.borrow()];
			}

			(Some(s), None) => {
				// This is the final string in the sequence.
				*obj = Value::String(s.borrow().to_owned());
				break;
			}

			// We were given an empty iterator.
			(None, _) => { break; }
		}
	}
}

fn get_path(
	obj: &Value,
	path: impl IntoIterator<Item=impl Borrow<str>>,
) -> Option<&Value> {
	let mut path = path.into_iter().peekable();

	if !obj.is_object() {
		return None;
	}

	if path.peek().is_none() {
		return None;
	}

	let mut value = Some(obj);
	loop {
		let key =
			match path.next() {
				Some(k) => k,
				None => break value,
			};

		match value.and_then(|v| v.get(key.borrow().to_owned())) {
			None => break None,
			Some(v@Value::Object(_)) => value = Some(v),
			v@Some(_) => {
				if path.peek().is_some() {
					break None
				} else {
					value = v;
				}
			},
		}
	}
}

#[cfg(test)]
mod test {
	use serde_json::{json, Value};

	use super::{get_path, set_path};

	mod get_path {
		use super::*;

		#[test]
		fn none_when_not_object() {
			let path = vec!["prop", "key"];

			assert_eq!(get_path(&Value::Null, path.clone()), None);
			assert_eq!(get_path(&json!(1), path.clone()), None);
			assert_eq!(get_path(&json!(1.2), path.clone()), None);
			assert_eq!(get_path(&json!(true), path.clone()), None);
			assert_eq!(get_path(&json!("value"), path.clone()), None);
		}

		#[test]
		fn none_when_path_not_found() {
			assert_eq!(get_path(&json!({"a": {"b": {"c": 1}}}), vec!["c", "b", "a"]), None);
			assert_eq!(get_path(&json!({"a": {"b": {"c": 1}}}), vec!["z", "y", "x"]), None);
		}

		#[test]
		fn none_when_path_doesnt_end() {
			assert_eq!(get_path(&json!({"a": {"b": {"c": 1}}}), vec!["a", "b", "c", "d"]), None);
		}

		#[test]
		fn none_when_doesnt_find_object() {
			assert_eq!(get_path(&json!({"a": 1}), vec!["a", "b"]), None);
			assert_eq!(get_path(&json!({"a": {"b": 1}}), vec!["a", "b", "c"]), None);
		}

		#[test]
		fn finds_in_depth() {
			assert_eq!(get_path(&json!({"a": {"b": {"c": 1}}}), vec!["a", "b", "c"]), Some(&json!(1)));
		}
	}

	mod set_path {
		use super::*;

		#[test]
		fn changes_only_null_and_object() {
			let path = vec!["prop", "key"];
			vec!

			assert_eq!(get_path(&Value::Null, path.clone()), None);
			assert_eq!(get_path(&json!(1), path.clone()), None);
			assert_eq!(get_path(&json!(1.2), path.clone()), None);
			assert_eq!(get_path(&json!(true), path.clone()), None);
			assert_eq!(get_path(&json!("value"), path.clone()), None);

		}
	}
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
