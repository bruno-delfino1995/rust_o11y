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
