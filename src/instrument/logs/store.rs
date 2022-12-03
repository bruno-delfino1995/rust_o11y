use self::Goal::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ops::Deref;
use std::ops::DerefMut;
use tracing::field::Visit;

pub enum Goal<F> {
	Hit,
	Miss(F),
}

impl<F> Goal<F> {
	pub fn or_else<T, O: FnOnce(F) -> Goal<T>>(self, op: O) -> Goal<T> {
		match self {
			Hit => Hit,
			Miss(input) => op(input),
		}
	}

	pub fn map_miss<T, O: FnOnce(F) -> T>(self, op: O) -> Goal<T> {
		match self {
			Hit => Hit,
			Miss(input) => Miss(op(input)),
		}
	}
}

pub type PortBy<'a> = Box<dyn Fn(&str) -> Option<String> + 'a>;

#[derive(Default, Clone)]
pub struct Store(BTreeMap<String, Value>);

impl Store {
	pub fn new() -> Self {
		Store(BTreeMap::new())
	}

	pub fn port(&mut self, from: &mut Self, keys: Vec<&str>) -> Goal<&mut Self> {
		let mut ported = Miss(());

		for k in keys {
			match from.remove_entry(k) {
				Some((k, v)) => {
					self.0.insert(k, v);
					ported = Hit;
				}
				None => continue,
			};
		}

		ported.map_miss(|_| self)
	}

	pub fn port_by(&mut self, from: &mut Self, predicate: PortBy) -> Goal<&mut Self> {
		let mut ported = Miss(());

		from.retain(|k, v| match predicate(k) {
			None => true,
			Some(nk) => {
				self.insert(nk, v.clone());
				ported = Hit;

				false
			}
		});

		ported.map_miss(|_| self)
	}

	pub fn port_all(&mut self, from: &mut Self) {
		self.0.append(&mut from.0);
	}

	pub fn push(&mut self, field: &str, from: Self) {
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
