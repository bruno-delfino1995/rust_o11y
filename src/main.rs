#![feature(panic_info_message)]
#![allow(unused)]

mod instrument;

use axum::Router;
use futures::future;
use tracing::{error, info, info_span, Span};

// #[tokio::main]
// async fn main() {
// 	instrument::init();
//
// 	let outer_span = info_span!("outer", level = 0);
// 	let _outer_entered = outer_span.enter();
//
// 	let inner_span = info_span!("inner", level = 1);
// 	let _inner_entered = inner_span.enter();
//
// 	info!(a_bool = true, answer = 42, message = "first example");
// }

#[tokio::main]
async fn main() {
	instrument::init();

	let application = {
		let router = instrument::axum::collectors(router::create());

		Box::pin(server::init(router, 3000))
	};

	let monitoring = {
		let router = instrument::axum::reporters(Router::new());

		Box::pin(server::init(router, 8000))
	};

	let (result, failed_future_index, _) = future::select_all(vec![application, monitoring]).await;

	match failed_future_index {
		0 => error!("http server aborted: {:?}", result),
		1 => error!("monitoring server aborted: {:?}", result),
		_ => unreachable!("unreachable code. a catastrophic error happened"),
	}

	instrument::stop();
}

mod router {
	use axum::{routing::get, Router};
	use reqwest_middleware::{ClientBuilder, Extension};
	use reqwest_tracing::{OtelName, TracingMiddleware};
	use tracing::info;

	pub fn create() -> Router {
		Router::new()
			.route("/", get(root))
			.route("/hello", get(hello))
			.route("/explode", get(explode))
	}

	async fn root() -> String {
		let reqwest_client = reqwest::Client::builder().build().unwrap();
		let client = ClientBuilder::new(reqwest_client)
			.with_init(Extension(OtelName("localhost".into())))
			.with(TracingMiddleware::default())
			.build();

		let response = client
			.get("http://localhost:3000/explode")
			.send()
			.await
			.unwrap()
			.text()
			.await
			.unwrap();

		format!("Got response: {:?}", response)
	}

	async fn hello() -> &'static str {
		"Hello, World!"
	}

	async fn explode() {
		info!("Are you serious?");

		panic!("Why you hate me?");
	}
}

mod server {
	use axum::Router;
	use std::net::SocketAddr;
	use tracing::info;

	pub async fn init(router: Router, port: u16) {
		let addr = SocketAddr::from(([0, 0, 0, 0], port));

		info!(
			port = addr.port(),
			ip = addr.ip().to_string(),
			"starting server"
		);

		axum::Server::bind(&addr)
			.serve(router.into_make_service())
			.with_graceful_shutdown(shutdown_signal())
			.await
			.expect("server error");
	}

	async fn shutdown_signal() {
		tokio::signal::ctrl_c()
			.await
			.expect("failed to install CTRL+C signal handler");
	}
}
