use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use metrics_exporter_prometheus::PrometheusBuilder;
use once_cell::sync::OnceCell;

pub(crate) static HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

pub fn init() {
	HANDLE.get_or_init(|| {
		PrometheusBuilder::new()
			.install_recorder()
			.expect("Unable to install prometheus recorder")
	});
}
