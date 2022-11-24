#![feature(panic_info_message)]

mod instrument;

#[tokio::main]
async fn main() {
	instrument::init();

	let router = {
		let router = router::create();

		instrument::axum::setup(router)
	};

	server::init(router).await;

	instrument::stop();
}

mod router {
	use axum::{routing::get, Router};
	use reqwest_middleware::{ClientBuilder, Extension};
	use reqwest_tracing::{OtelName, TracingMiddleware};

	pub fn create() -> Router {
		Router::new()
			.route("/", get(root))
			.route("/health", get(health))
			.route("/hello", get(hello))
	}

	async fn root() -> String {
		let reqwest_client = reqwest::Client::builder().build().unwrap();
		let client = ClientBuilder::new(reqwest_client)
			.with_init(Extension(OtelName("localhost".into())))
			.with(TracingMiddleware::default())
			.build();

		let response = client
			.get("http://localhost:3000/health")
			.send()
			.await
			.unwrap()
			.text()
			.await
			.unwrap();

		format!("Got response: {:?}", response)
	}

	async fn health() -> &'static str {
		"I'm healthy"
	}

	async fn hello() -> &'static str {
		"Hello, World!"
	}
}

mod server {
	use axum::Router;
	use std::net::SocketAddr;
	use tracing::info;

	pub async fn init(router: Router) {
		let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
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
