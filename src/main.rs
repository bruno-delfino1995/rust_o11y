#![feature(panic_info_message)]

use axum::{routing::get, Router};
use std::net::SocketAddr;
use std::panic;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
	setup_tracing();

	let router = create_router();
	setup_server(router).await;
}

fn setup_tracing() {
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

	tracing_subscriber::registry()
		.with(max_level)
		.with(format)
		.init();

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

fn create_router() -> Router {
	Router::new().route("/", get(|| async { "Hello, World!" }))
}

async fn setup_server(router: Router) {
	let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
	info!(
		port = addr.port(),
		ip = addr.ip().to_string(),
		"starting server"
	);

	axum::Server::bind(&addr)
		.serve(router.into_make_service())
		.await
		.expect("unable to start server");
}
