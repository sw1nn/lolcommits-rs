# Prometheus Metrics Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Prometheus-compatible metrics to lolcommitsd, following the same pattern established in sw1nn-pkg-repo.

**Architecture:** Single `src/metrics.rs` module with: `install_recorder()` returning `PrometheusHandle`, upfront `register_descriptions()`, `ScopedTimer` RAII guard, thin helper functions for counters/gauges, `http_metrics_layer` as axum middleware. `/metrics` route merged separately via closure capture (not AppState). Endpoint normalisation for low-cardinality labels.

**Tech Stack:** `metrics` 0.24, `metrics-exporter-prometheus` 0.16

---

### Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add to `[dependencies]`:**
```toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
```

**Step 2:** `cargo check` — SUCCESS

**Step 3:** Commit: `feat(metrics): add metrics and prometheus exporter dependencies`

---

### Task 2: Expose RUSTC_VERSION from build.rs

**Files:**
- Modify: `build.rs`

**Step 1:** Add before the closing brace of `main()`:
```rust
let rustc_output = Command::new("rustc")
    .arg("--version")
    .output()
    .expect("Failed to run rustc --version");
let rustc_version = String::from_utf8_lossy(&rustc_output.stdout).trim().to_string();
println!("cargo:rustc-env=RUSTC_VERSION={rustc_version}");
```

**Step 2:** `cargo check` — SUCCESS

**Step 3:** Commit: `build: expose RUSTC_VERSION env var for metrics`

---

### Task 3: Create metrics module

**Files:**
- Create: `src/metrics.rs`
- Modify: `src/lib.rs` (add `pub mod metrics;`)

**Step 1:** Create `src/metrics.rs` with the full module following the sw1nn-pkg-repo pattern:

```rust
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
    describe_gauge!("lolcommits_images_total", "Total number of images in gallery");
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
    // All lolcommits routes are static, no dynamic segments to normalise
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
```

**Step 2:** Add `pub mod metrics;` to `src/lib.rs`

**Step 3:** `cargo check` — SUCCESS

**Step 4:** Commit: `feat(metrics): add metrics module with recorder, descriptions, helpers, and middleware`

---

### Task 4: Wire up metrics in lolcommitsd and server

**Files:**
- Modify: `src/bin/lolcommitsd.rs`
- Modify: `src/server.rs`

**Step 1:** In `src/bin/lolcommitsd.rs`, after `init_tracing_with_output(log_output)`:
```rust
let metrics_handle = sw1nn_lolcommits_rs::metrics::install_recorder();
```
Pass `metrics_handle` to `server::create_router`.

**Step 2:** In `src/server.rs`, update `create_router` signature:
```rust
pub fn create_router(
    data_home: std::path::PathBuf,
    metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
) -> Router {
```

Add `/metrics` route merged separately (outside middleware), same pattern as pkg-repo:
```rust
let metrics_routes = Router::new().route(
    "/metrics",
    get(move || std::future::ready(metrics_handle.render())),
);
```

Add middleware layer:
```rust
.layer(axum::middleware::from_fn(crate::metrics::http_metrics_layer))
```

**Step 3:** `cargo check` — SUCCESS

**Step 4:** Commit: `feat(metrics): wire up /metrics endpoint and HTTP middleware`

---

### Task 5: Instrument upload pipeline and image processing

**Files:**
- Modify: `src/server.rs`

**Step 1:** In `upload_handler`, after parsing succeeds:
```rust
crate::metrics::record_upload("accepted");
```

**Step 2:** In `process_image_async`:
- On duplicate skip: `crate::metrics::record_upload("duplicate_skipped");`
- Wrap image processing (decode through save) with `let _timer = crate::metrics::ScopedTimer::image_processing();`
- On success: `crate::metrics::record_upload("processed");`
- On error (wrap the spawn body): `crate::metrics::record_upload("failed");`

**Step 3:** `cargo check` — SUCCESS

**Step 4:** Commit: `feat(metrics): instrument upload pipeline with counters and processing histogram`

---

### Task 6: Add gallery and SSE gauges

**Files:**
- Modify: `src/server.rs`

**Step 1:** After `initialize_revision_cache` in `create_router`:
```rust
crate::metrics::set_images_total(cache.len());
crate::metrics::set_revision_cache_size(cache.len());
```

**Step 2:** After successful save in `process_image_async`, after updating revision_cache:
```rust
crate::metrics::set_revision_cache_size(cache.len());
crate::metrics::increment_images_total();
```

**Step 3:** In `sse_handler`, increment on connect and decrement via drop guard:
```rust
crate::metrics::increment_sse_connections();
// In the stream, after the loop ends:
crate::metrics::decrement_sse_connections();
```

**Step 4:** `cargo check` — SUCCESS

**Step 5:** Commit: `feat(metrics): add gallery size, cache size, and SSE connection gauges`

---

### Task 7: Final verification

**Step 1:** `cargo check`
**Step 2:** `cargo test`
**Step 3:** `cargo fmt`
**Step 4:** `cargo clippy`

All must pass cleanly.
