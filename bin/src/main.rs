mod router;
mod server;

use axum::Router;
use futures::future;
use tracing::error;

#[tokio::main]
async fn main() {
	let _guard = instrument::init();

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
