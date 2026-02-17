// Simple standalone metrics for Prometheus and JSON dashboard
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub static METRICS: Lazy<SimpleMetrics> = Lazy::new(|| SimpleMetrics::new());

// Track transaction timestamps for TPS calculation
const TPS_1M_WINDOW_SECS: u64 = 60; // 1 minute rolling window
const TPS_5M_WINDOW_SECS: u64 = 300; // 5 minute rolling window

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

/// Get the current process CPU time on Windows using GetProcessTimes.
/// Returns total (kernel + user) time in 100-nanosecond intervals.
#[cfg(windows)]
fn win_process_cpu_time() -> u64 {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            hProcess: *mut std::ffi::c_void,
            lpCreationTime: *mut FileTime,
            lpExitTime: *mut FileTime,
            lpKernelTime: *mut FileTime,
            lpUserTime: *mut FileTime,
        ) -> i32;
    }

    unsafe {
        let handle = GetCurrentProcess();
        let mut creation = FileTime::default();
        let mut exit = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();

        if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0 {
            let kernel_time = (kernel.high as u64) << 32 | kernel.low as u64;
            let user_time = (user.high as u64) << 32 | user.low as u64;
            kernel_time + user_time
        } else {
            0
        }
    }
}

impl SimpleMetrics {
    pub fn new() -> Self {
        let cpu_usage = Arc::new(AtomicU64::new(0));

        let cpu_clone = cpu_usage.clone();
        std::thread::spawn(move || {
            let cpu_count = num_cpus::get().max(1) as f64;
            loop {
                // On supported platforms, try to read process CPU time
                #[cfg(unix)]
                let load = {
                    use std::io::Read;
                    // Read /proc/self/stat for process CPU time
                    let mut stat = String::new();
                    if let Ok(mut f) = std::fs::File::open("/proc/self/stat") {
                        let _ = f.read_to_string(&mut stat);
                    }
                    let fields: Vec<&str> = stat.split_whitespace().collect();
                    let utime: u64 = fields.get(13).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let stime: u64 = fields.get(14).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let total_start = utime + stime;

                    std::thread::sleep(Duration::from_secs(5));

                    let mut stat2 = String::new();
                    if let Ok(mut f) = std::fs::File::open("/proc/self/stat") {
                        let _ = f.read_to_string(&mut stat2);
                    }
                    let fields2: Vec<&str> = stat2.split_whitespace().collect();
                    let utime2: u64 = fields2.get(13).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let stime2: u64 = fields2.get(14).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let total_end = utime2 + stime2;

                    let ticks_per_sec = 100.0;
                    let cpu_ms = ((total_end - total_start) as f64 / ticks_per_sec) * 1000.0;
                    let elapsed_ms = 5000.0f64;
                    ((cpu_ms / elapsed_ms / cpu_count) * 100.0).min(100.0) as u64
                };

                // On Windows, use GetProcessTimes for real CPU measurement
                #[cfg(windows)]
                let load = {
                    let t1 = win_process_cpu_time();
                    std::thread::sleep(Duration::from_secs(5));
                    let t2 = win_process_cpu_time();

                    // GetProcessTimes returns 100ns intervals
                    // delta is in 100ns units, convert to ms: delta / 10_000
                    let delta_100ns = t2.saturating_sub(t1);
                    let cpu_ms = delta_100ns as f64 / 10_000.0;
                    let elapsed_ms = 5000.0f64;
                    ((cpu_ms / elapsed_ms / cpu_count) * 100.0).min(100.0) as u64
                };

                // Fallback for other platforms
                #[cfg(not(any(unix, windows)))]
                let load = {
                    std::thread::sleep(Duration::from_secs(5));
                    0u64
                };

                cpu_clone.store(load, Ordering::Relaxed);
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

        // Prune old timestamps outside the 5-minute window (largest window we need)
        let cutoff = now_millis.saturating_sub(TPS_5M_WINDOW_SECS * 1000);
        while let Some(&front) = timestamps.front() {
            if front < cutoff {
                timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate transactions per second over a given window
    fn calculate_tps_for_window(&self, window_secs: u64) -> f64 {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let cutoff = now_millis.saturating_sub(window_secs * 1000);

        let timestamps = self.tx_timestamps.lock();
        let count = timestamps.iter().filter(|&&ts| ts >= cutoff).count();

        if count == 0 {
            return 0.0;
        }

        count as f64 / window_secs as f64
    }

    /// Calculate TPS over 1 minute
    pub fn calculate_tps_1m(&self) -> f64 {
        self.calculate_tps_for_window(TPS_1M_WINDOW_SECS)
    }

    /// Calculate TPS over 5 minutes
    pub fn calculate_tps_5m(&self) -> f64 {
        self.calculate_tps_for_window(TPS_5M_WINDOW_SECS)
    }

    /// Backwards-compatible alias
    pub fn calculate_tps(&self) -> f64 {
        self.calculate_tps_1m()
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Export metrics as JSON for the dashboard
    pub fn export_json(&self) -> serde_json::Value {
        let (active_conns, dedupe, peer_count) = crate::network::get_p2p_metrics();
        let tps_1m = self.calculate_tps_1m();
        let tps_5m = self.calculate_tps_5m();
        let mempool_count = crate::mempool::get_mempool_count();

        serde_json::json!({
            "tps_1m": tps_1m,
            "tps_5m": tps_5m,
            "transactions_total": self.transactions_processed.load(Ordering::Relaxed),
            "http_requests": self.http_requests.load(Ordering::Relaxed),
            "http_errors": self.http_errors.load(Ordering::Relaxed),
            "consensus_rounds": self.consensus_rounds.load(Ordering::Relaxed),
            "peer_connections": active_conns,
            "peer_count": peer_count,
            "dedupe_entries": dedupe,
            "uptime_secs": self.uptime_secs(),
            "mempool_count": mempool_count,
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
