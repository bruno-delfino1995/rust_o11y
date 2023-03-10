mod metrics;
mod traces;

use axum::Router;
use tower::ServiceBuilder;

pub fn collect_from(router: Router) -> Router {
	let metrics_layer = self::metrics::layer();
	let traces_layer = self::traces::layer();

	router.layer(
		ServiceBuilder::new()
			.layer(traces_layer)
			.layer(metrics_layer),
	)
}

pub fn report_at(router: Router) -> Router {
	self::metrics::route(router)
}
