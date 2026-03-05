use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::time::Instant;

/// Install the Prometheus metrics recorder and register all metric descriptions.
pub fn install_recorder() -> metrics_exporter_prometheus::PrometheusHandle {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    register_descriptions();

    gauge!(
        "lolcommits_build_info",
        "version" => env!("CARGO_PKG_VERSION"),
        "rustc_version" => env!("RUSTC_VERSION"),
    )
    .set(1.0);

    handle
}

fn register_descriptions() {
    describe_gauge!(
        "lolcommits_build_info",
        "Build information (version, rustc_version labels), always 1"
    );

    // Gauges
    describe_gauge!(
        "lolcommits_images_total",
        "Total number of images in gallery"
    );
    describe_gauge!(
        "lolcommits_revision_cache_size",
        "Number of revisions in dedup cache"
    );
    describe_gauge!(
        "lolcommits_sse_connections_active",
        "Number of active SSE connections"
    );

    // Counters
    describe_counter!("lolcommits_http_requests_total", "Total HTTP requests");
    describe_counter!(
        "lolcommits_uploads_total",
        "Total uploads by status (accepted, duplicate_skipped, processed, failed)"
    );

    // Histograms
    describe_histogram!(
        "lolcommits_http_request_duration_seconds",
        "HTTP request duration in seconds"
    );
    describe_histogram!(
        "lolcommits_image_processing_duration_seconds",
        "Image processing duration in seconds"
    );
}

/// RAII timer that records elapsed time to a histogram on drop.
pub struct ScopedTimer {
    start: Instant,
    name: &'static str,
    labels: Vec<(&'static str, String)>,
}

impl ScopedTimer {
    pub fn new(name: &'static str, labels: Vec<(&'static str, String)>) -> Self {
        Self {
            start: Instant::now(),
            name,
            labels,
        }
    }

    pub fn http_request(method: String, endpoint: String) -> Self {
        Self::new(
            "lolcommits_http_request_duration_seconds",
            vec![("method", method), ("endpoint", endpoint)],
        )
    }

    pub fn image_processing() -> Self {
        Self::new("lolcommits_image_processing_duration_seconds", vec![])
    }
}

impl Drop for ScopedTimer {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_secs_f64();
        let labels: Vec<metrics::Label> = self
            .labels
            .iter()
            .map(|(k, v)| metrics::Label::new(*k, v.clone()))
            .collect();
        histogram!(self.name, labels).record(elapsed);
    }
}

// -- Counter helpers --

pub fn record_http_request(method: &str, endpoint: &str, status: u16) {
    counter!(
        "lolcommits_http_requests_total",
        "method" => method.to_owned(),
        "endpoint" => endpoint.to_owned(),
        "status" => status.to_string()
    )
    .increment(1);
}

pub fn record_upload(status: &str) {
    counter!("lolcommits_uploads_total", "status" => status.to_owned()).increment(1);
}

// -- Gauge helpers --

pub fn set_images_total(count: usize) {
    gauge!("lolcommits_images_total").set(count as f64);
}

pub fn increment_images_total() {
    gauge!("lolcommits_images_total").increment(1.0);
}

pub fn set_revision_cache_size(count: usize) {
    gauge!("lolcommits_revision_cache_size").set(count as f64);
}

pub fn increment_sse_connections() {
    gauge!("lolcommits_sse_connections_active").increment(1.0);
}

pub fn decrement_sse_connections() {
    gauge!("lolcommits_sse_connections_active").decrement(1.0);
}

/// Normalise a request path into a low-cardinality endpoint label.
fn normalise_endpoint(path: &str) -> String {
    path.to_owned()
}

/// Axum middleware that records HTTP request count and duration.
pub async fn http_metrics_layer(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();

    if path == "/metrics" {
        return next.run(request).await;
    }

    let endpoint = normalise_endpoint(&path);
    let _timer = ScopedTimer::http_request(method.clone(), endpoint.clone());

    let response = next.run(request).await;

    let status = response.status().as_u16();
    record_http_request(&method, &endpoint, status);

    response
}
