use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use metrics_exporter_prometheus::PrometheusBuilder;
use once_cell::sync::OnceCell;

static HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

pub fn init() {
	HANDLE.get_or_init(|| {
		PrometheusBuilder::new()
			 .install_recorder()
			 .expect("Unable to install prometheus recorder")
	});
}

pub mod axum {
	use super::HANDLE;

	use axum_prometheus::PrometheusMetricLayer;
	use axum::{routing::get, Router};

	pub fn layer() -> PrometheusMetricLayer {
		PrometheusMetricLayer::new()
	}

	pub fn route(router: Router) -> Router {
		router.route("/metrics", get(|| async { HANDLE.get().expect("Should have initialized metrics module first").render() }))
	}
}
