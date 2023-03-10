use axum::{routing::get, Router};
use instrument::http::client;
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use tracing::info;

pub fn create() -> Router {
	Router::new()
		.route("/", get(root))
		.route("/hello", get(hello))
		.route("/explode", get(explode))
}

async fn root() -> String {
	let reqwest_client = Client::new();

	let client = client::decorate(ClientBuilder::new(reqwest_client)).build();

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
