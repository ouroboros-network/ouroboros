// Simple standalone metrics for Prometheus and JSON dashboard
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub static METRICS: Lazy<SimpleMetrics> = Lazy::new(|| SimpleMetrics::new());

// Track transaction timestamps for TPS calculation
const TPS_WINDOW_SECS: u64 = 60; // 1 minute rolling window

pub struct SimpleMetrics {
    pub http_requests: Arc<AtomicU64>,
    pub http_errors: Arc<AtomicU64>,
    pub consensus_rounds: Arc<AtomicU64>,
    pub transactions_processed: Arc<AtomicU64>,
    pub cpu_usage: Arc<AtomicU64>,
    // Rolling window of transaction timestamps (unix millis)
    tx_timestamps: Arc<Mutex<VecDeque<u64>>>,
    start_time: Instant,
}

impl SimpleMetrics {
    pub fn new() -> Self {
        let cpu_usage = Arc::new(AtomicU64::new(0));
        
        // Spawn background CPU sampler
        let cpu_clone = cpu_usage.clone();
        std::thread::spawn(move || {
            let _last_sample = Instant::now();
            loop {
                // Very basic mock CPU sampling for now - can be improved with sysinfo crate
                // For now, we simulate load based on transaction throughput
                let load = (rand::random::<u8>() % 15) as u64; // 0-15% base load
                cpu_clone.store(load, Ordering::Relaxed);
                std::thread::sleep(Duration::from_secs(5));
            }
        });

        Self {
            http_requests: Arc::new(AtomicU64::new(0)),
            http_errors: Arc::new(AtomicU64::new(0)),
            consensus_rounds: Arc::new(AtomicU64::new(0)),
            transactions_processed: Arc::new(AtomicU64::new(0)),
            cpu_usage,
            tx_timestamps: Arc::new(Mutex::new(VecDeque::with_capacity(10000))),
            start_time: Instant::now(),
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

    /// Record a transaction for TPS tracking
    pub fn record_transaction(&self) {
        self.transactions_processed.fetch_add(1, Ordering::Relaxed);

        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut timestamps = self.tx_timestamps.lock();
        timestamps.push_back(now_millis);

        // Prune old timestamps outside the window
        let cutoff = now_millis.saturating_sub(TPS_WINDOW_SECS * 1000);
        while let Some(&front) = timestamps.front() {
            if front < cutoff {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate transactions per second over the rolling window
    pub fn calculate_tps(&self) -> f64 {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let cutoff = now_millis.saturating_sub(TPS_WINDOW_SECS * 1000);

        let mut timestamps = self.tx_timestamps.lock();

        // Prune old timestamps
        while let Some(&front) = timestamps.front() {
            if front < cutoff {
                timestamps.pop_front();
            } else {
                break;
            }
        }

        let count = timestamps.len();
        if count == 0 {
            return 0.0;
        }

        // TPS = transactions in window / window duration in seconds
        count as f64 / TPS_WINDOW_SECS as f64
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Export metrics as JSON for the dashboard
    pub fn export_json(&self) -> serde_json::Value {
        let (active_conns, dedupe, peer_count) = crate::network::get_p2p_metrics();
        let tps = self.calculate_tps();

        serde_json::json!({
            "tps_1m": tps,
            "tps_5m": tps, // Using same value for now
            "transactions_total": self.transactions_processed.load(Ordering::Relaxed),
            "http_requests": self.http_requests.load(Ordering::Relaxed),
            "http_errors": self.http_errors.load(Ordering::Relaxed),
            "consensus_rounds": self.consensus_rounds.load(Ordering::Relaxed),
            "peer_connections": active_conns,
            "peer_count": peer_count,
            "dedupe_entries": dedupe,
            "uptime_secs": self.uptime_secs(),
            "mempool_count": 0, // TODO: Get from actual mempool
            "block_height": self.consensus_rounds.load(Ordering::Relaxed),
            "sync_percent": 100.0
        })
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
