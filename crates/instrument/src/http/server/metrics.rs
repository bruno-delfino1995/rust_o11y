use crate::metrics::HANDLE;

use axum::{routing::get, Router};
use axum_prometheus::PrometheusMetricLayer;

pub fn layer() -> PrometheusMetricLayer {
	PrometheusMetricLayer::new()
}

pub fn route(router: Router) -> Router {
	router.route(
		"/metrics",
		get(|| async {
			HANDLE
				.get()
				.expect("Should have initialized metrics module first")
				.render()
		}),
	)
}
