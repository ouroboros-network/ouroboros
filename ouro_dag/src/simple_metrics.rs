// Simple standalone metrics for Prometheus
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub static METRICS: Lazy<SimpleMetrics> = Lazy::new(|| SimpleMetrics::new());

pub struct SimpleMetrics {
    pub http_requests: Arc<AtomicU64>,
    pub http_errors: Arc<AtomicU64>,
    pub consensus_rounds: Arc<AtomicU64>,
}

impl SimpleMetrics {
    pub fn new() -> Self {
        Self {
            http_requests: Arc::new(AtomicU64::new(0)),
            http_errors: Arc::new(AtomicU64::new(0)),
            consensus_rounds: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn inc_http_requests(&self) {
        self.http_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_http_errors(&self) {
        self.http_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_consensus_rounds(&self) {
        self.consensus_rounds.fetch_add(1, Ordering::Relaxed);
    }

    pub fn export_prometheus(&self) -> String {
        let (active_conns, dedupe, peer_count) = crate::network::get_p2p_metrics();

        format!(
            "# HELP http_requests_total Total HTTP requests\n\
             # TYPE http_requests_total counter\n\
             http_requests_total {}\n\
             \n\
             # HELP http_errors_total Total HTTP errors\n\
             # TYPE http_errors_total counter\n\
             http_errors_total {}\n\
             \n\
             # HELP consensus_rounds_total Total consensus rounds\n\
             # TYPE consensus_rounds_total counter\n\
             consensus_rounds_total {}\n\
             \n\
             # HELP peer_connections_active Active P2P connections\n\
             # TYPE peer_connections_active gauge\n\
             peer_connections_active {}\n\
             \n\
             # HELP peer_count_total Total known peers\n\
             # TYPE peer_count_total gauge\n\
             peer_count_total {}\n\
             \n\
             # HELP message_dedupe_entries Dedupe cache entries\n\
             # TYPE message_dedupe_entries gauge\n\
             message_dedupe_entries {}\n",
            self.http_requests.load(Ordering::Relaxed),
            self.http_errors.load(Ordering::Relaxed),
            self.consensus_rounds.load(Ordering::Relaxed),
            active_conns,
            peer_count,
            dedupe
        )
    }
}
