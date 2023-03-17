mod router;
mod server;

use axum::Router;
use futures::future;
use tracing::error;

#[tokio::main]
async fn main() {
	let config = config();

	let _guard = instrument::init(instrument::Options {
		level: &config.log_level,
		service: "gollum",
		version: "0.0.0",
		exporter: &config.otlp_exporter,
	});

	let application = {
		let router = instrument::http::server::collect_from(router::create());

		Box::pin(server::init(router, 3000))
	};

	let monitoring = {
		let router = instrument::http::server::report_at(Router::new());

		Box::pin(server::init(router, 8000))
	};

	let (result, failed_future_index, _) = future::select_all(vec![application, monitoring]).await;

	match failed_future_index {
		0 => error!("http server aborted: {:?}", result),
		1 => error!("monitoring server aborted: {:?}", result),
		_ => unreachable!("unreachable code. a catastrophic error happened"),
	}
}

struct Config {
	log_level: String,
	otlp_exporter: String,
}

fn config() -> Config {
	use std::env::var;

	Config {
		log_level: var("LOG_LEVEL").expect("$LOG_LEVEL is required"),
		otlp_exporter: var("OTLP_EXPORTER").expect("$OTLP_EXPORTER is required"),
	}
}
