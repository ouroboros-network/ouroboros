// src/cli_dashboard.rs
//! CLI Dashboard - Terminal-based node monitoring and control
//!
//! Provides beautiful terminal output for node status, peers, consensus, etc.
//! Uses crossterm for flicker-free rendering in an alternate screen buffer.

use std::io::{self, Write};
use crossterm::{
    cursor,
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

/// Enter alternate screen and hide cursor for live dashboard mode.
/// Call `leave_dashboard_mode()` before exiting to restore terminal state.
pub fn enter_dashboard_mode() {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, EnterAlternateScreen, cursor::Hide);
}

/// Restore terminal: show cursor and leave alternate screen.
pub fn leave_dashboard_mode() {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
}

/// Check if Ctrl+C was pressed (non-blocking).
/// Returns true if the user wants to quit.
pub fn poll_ctrl_c() -> bool {
    if crossterm::event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
        if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
            return key.code == crossterm::event::KeyCode::Char('c')
                && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL);
        }
    }
    false
}

/// ANSI color codes
pub mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";

    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";

    pub const BG_RED: &str = "\x1b[41m";
    pub const BG_GREEN: &str = "\x1b[42m";
    pub const BG_YELLOW: &str = "\x1b[43m";
    pub const BG_BLUE: &str = "\x1b[44m";
}

/// Box drawing characters
pub mod box_chars {
    pub const TOP_LEFT: &str = "┌";
    pub const TOP_RIGHT: &str = "┐";
    pub const BOTTOM_LEFT: &str = "└";
    pub const BOTTOM_RIGHT: &str = "┘";
    pub const HORIZONTAL: &str = "─";
    pub const VERTICAL: &str = "│";
    pub const T_DOWN: &str = "┬";
    pub const T_UP: &str = "┴";
    pub const T_RIGHT: &str = "├";
    pub const T_LEFT: &str = "┤";
    pub const CROSS: &str = "┼";
}

/// Node status enum
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeStatus {
    Running,
    Stopped,
    Syncing,
    Error,
}

impl NodeStatus {
    pub fn color(&self) -> &'static str {
        match self {
            NodeStatus::Running => colors::GREEN,
            NodeStatus::Stopped => colors::RED,
            NodeStatus::Syncing => colors::BLUE,
            NodeStatus::Error => colors::RED,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            NodeStatus::Running => "●",
            NodeStatus::Stopped => "○",
            NodeStatus::Syncing => "◐",
            NodeStatus::Error => "✖",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NodeStatus::Running => "Running",
            NodeStatus::Stopped => "Stopped",
            NodeStatus::Syncing => "Syncing",
            NodeStatus::Error => "Error",
        }
    }
}

/// Status data for the dashboard
#[derive(Debug, Clone)]
pub struct DashboardData {
    pub node_name: String,
    pub status: NodeStatus,
    pub role: String,
    pub version: String,
    pub difficulty: String,
    pub uptime_secs: u64,

    // Consensus
    pub view: u64,
    pub leader: String,
    pub last_block_height: u64,
    pub last_block_time: String,
    pub highest_qc: u64,

    // Sync
    pub local_height: u64,
    pub network_tip: u64,
    pub sync_percent: f64,
    pub eta_secs: Option<u64>,

    // Peers
    pub peer_count: u32,
    pub top_peers: Vec<PeerInfo>,

    // Mempool
    pub mempool_tx_count: u32,
    pub mempool_avg_age_secs: f64,
    pub tps_1m: f64,
    pub tps_5m: f64,

    // Resources
    pub cpu_percent: f64,
    pub mem_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub net_in_kbps: f64,
    pub net_out_kbps: f64,

    // Wallet
    pub wallet_address: Option<String>,
    pub wallet_balance: Option<u64>,

    // Alerts
    pub alerts: Vec<Alert>,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub addr: String,
    pub role: String,
    pub latency_ms: u32,
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

impl AlertLevel {
    pub fn color(&self) -> &'static str {
        match self {
            AlertLevel::Info => colors::BLUE,
            AlertLevel::Warning => colors::YELLOW,
            AlertLevel::Critical => colors::RED,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            AlertLevel::Info => "ℹ",
            AlertLevel::Warning => "⚠",
            AlertLevel::Critical => "✖",
        }
    }
}

impl Default for DashboardData {
    fn default() -> Self {
        Self {
            node_name: "node-1".to_string(),
            status: NodeStatus::Stopped,
            role: "unknown".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            difficulty: "small".to_string(),
            uptime_secs: 0,
            view: 0,
            leader: "unknown".to_string(),
            last_block_height: 0,
            last_block_time: "N/A".to_string(),
            highest_qc: 0,
            local_height: 0,
            network_tip: 0,
            sync_percent: 0.0,
            eta_secs: None,
            peer_count: 0,
            top_peers: vec![],
            mempool_tx_count: 0,
            mempool_avg_age_secs: 0.0,
            tps_1m: 0.0,
            tps_5m: 0.0,
            cpu_percent: 0.0,
            mem_mb: 0,
            disk_used_gb: 0.0,
            disk_total_gb: 0.0,
            net_in_kbps: 0.0,
            net_out_kbps: 0.0,
            wallet_address: None,
            wallet_balance: None,
            alerts: vec![],
        }
    }
}

/// Format uptime in human-readable format
pub fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

/// Format bytes in human-readable format
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Create a progress bar
pub fn progress_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    format!(
        "{}{}{}{}{}",
        colors::GREEN,
        "█".repeat(filled),
        colors::DIM,
        "░".repeat(empty),
        colors::RESET
    )
}

/// Draw a horizontal line
pub fn horizontal_line(width: usize) -> String {
    box_chars::HORIZONTAL.repeat(width)
}

/// Print the main dashboard (live-updating mode).
/// Assumes alternate screen is already entered via `enter_dashboard_mode()`.
/// Uses cursor repositioning to redraw in-place without scrollback pollution.
pub fn print_dashboard(data: &DashboardData) {
    let mut stdout = io::stdout();
    // Move cursor to top-left — no screen clear needed since we overwrite + clear remainders
    let _ = execute!(stdout, cursor::MoveTo(0, 0));
    print_dashboard_lines(data, &mut stdout);
    // Clear everything below the dashboard to remove stale content from previous renders
    let _ = execute!(stdout, terminal::Clear(ClearType::FromCursorDown));
    let _ = stdout.flush();
}

/// Print dashboard without cursor tricks (for --once mode)
pub fn print_dashboard_once(data: &DashboardData) {
    let mut stdout = io::stdout();
    print_dashboard_lines(data, &mut stdout);
    let _ = stdout.flush();
}

/// Write a line and clear to end-of-line to erase leftover chars from previous renders.
/// Uses \r\n for raw-mode compatibility (raw mode doesn't translate \n to \r\n).
macro_rules! dash_line {
    ($out:expr, $($arg:tt)*) => {{
        write!($out, $($arg)*).ok();
        execute!($out, terminal::Clear(ClearType::UntilNewLine)).ok();
        write!($out, "\r\n").ok();
    }};
}

fn print_dashboard_lines(data: &DashboardData, out: &mut impl Write) {
    let width = 70;

    // Header
    dash_line!(out, "{}{}{}",
        box_chars::TOP_LEFT,
        horizontal_line(width - 2),
        box_chars::TOP_RIGHT
    );

    let title = format!(" OUROBOROS NODE - {} ", data.node_name);
    let padding = (width - 2 - title.len()) / 2;
    dash_line!(out, "{}{}{}{}{}{}{}{}",
        box_chars::VERTICAL,
        " ".repeat(padding),
        colors::BOLD,
        colors::CYAN,
        title,
        colors::RESET,
        " ".repeat(width - 2 - padding - title.len()),
        box_chars::VERTICAL
    );

    // Status line
    dash_line!(out, "{}{}{}",
        box_chars::T_RIGHT,
        horizontal_line(width - 2),
        box_chars::T_LEFT
    );

    let status_visible = format!(
        " Status: {} {}   Role: {}   Difficulty: {}   Uptime: {}",
        data.status.symbol(),
        data.status.label(),
        data.role,
        data.difficulty,
        format_uptime(data.uptime_secs)
    );
    let status_str = format!(
        " Status: {}{} {}{}   Role: {}   Difficulty: {}   Uptime: {}",
        data.status.color(),
        data.status.symbol(),
        data.status.label(),
        colors::RESET,
        data.role,
        data.difficulty,
        format_uptime(data.uptime_secs)
    );
    let visible_len = status_visible.len();
    let pad = if visible_len < width - 2 { width - 2 - visible_len } else { 0 };
    dash_line!(out, "{}{}{}{}",
        box_chars::VERTICAL,
        status_str,
        " ".repeat(pad),
        box_chars::VERTICAL
    );

    // Divider
    dash_line!(out, "{}{}{}",
        box_chars::T_RIGHT,
        horizontal_line(width - 2),
        box_chars::T_LEFT
    );

    // Consensus section
    dash_line!(out, "{} {}CONSENSUS{}{}",
        box_chars::VERTICAL,
        colors::BOLD,
        colors::RESET,
        " ".repeat(width - 12)
    );
    dash_line!(out, "{} View: {:<8} Leader: {:<15} Last Block: {} ({})",
        box_chars::VERTICAL,
        data.view,
        data.leader,
        data.last_block_height,
        data.last_block_time
    );

    // Sync section
    dash_line!(out, "{}{}{}",
        box_chars::T_RIGHT,
        horizontal_line(width - 2),
        box_chars::T_LEFT
    );

    dash_line!(out, "{} {}SYNC PROGRESS{}{}",
        box_chars::VERTICAL,
        colors::BOLD,
        colors::RESET,
        " ".repeat(width - 16)
    );

    let sync_bar = progress_bar(data.sync_percent, 30);
    let eta_str = match data.eta_secs {
        Some(secs) if secs > 0 => format!("ETA: {}", format_uptime(secs)),
        _ => "Synced".to_string(),
    };
    dash_line!(out, "{} {} {:.1}% {} {}/{} blocks {}",
        box_chars::VERTICAL,
        sync_bar,
        data.sync_percent,
        " ".repeat(2),
        data.local_height,
        data.network_tip,
        eta_str
    );

    // Peers & Mempool section
    dash_line!(out, "{}{}{}",
        box_chars::T_RIGHT,
        horizontal_line(width - 2),
        box_chars::T_LEFT
    );

    dash_line!(out, "{} {}PEERS:{} {} connected     {}MEMPOOL:{} {} txs     {}TPS:{} {:.1}/s (1m) {:.1}/s (5m)",
        box_chars::VERTICAL,
        colors::BOLD,
        colors::RESET,
        data.peer_count,
        colors::BOLD,
        colors::RESET,
        data.mempool_tx_count,
        colors::BOLD,
        colors::RESET,
        data.tps_1m,
        data.tps_5m
    );

    // Resources section
    dash_line!(out, "{}{}{}",
        box_chars::T_RIGHT,
        horizontal_line(width - 2),
        box_chars::T_LEFT
    );

    let cpu_color = if data.cpu_percent > 85.0 {
        colors::RED
    } else if data.cpu_percent > 60.0 {
        colors::YELLOW
    } else {
        colors::GREEN
    };

    let mem_color = if data.mem_mb > 1024 {
        colors::YELLOW
    } else {
        colors::GREEN
    };

    let disk_percent = if data.disk_total_gb > 0.0 {
        (data.disk_used_gb / data.disk_total_gb) * 100.0
    } else {
        0.0
    };
    let disk_color = if disk_percent > 90.0 {
        colors::RED
    } else if disk_percent > 75.0 {
        colors::YELLOW
    } else {
        colors::GREEN
    };

    dash_line!(out, "{} CPU: {}{:.1}%{}  MEM: {}{}MB{}  DISK: {}{:.1}/{:.1}GB{}  NET: ↓{:.0} ↑{:.0} KB/s",
        box_chars::VERTICAL,
        cpu_color,
        data.cpu_percent,
        colors::RESET,
        mem_color,
        data.mem_mb,
        colors::RESET,
        disk_color,
        data.disk_used_gb,
        data.disk_total_gb,
        colors::RESET,
        data.net_in_kbps,
        data.net_out_kbps
    );

    // Wallet section (if linked)
    if let Some(ref addr) = data.wallet_address {
        dash_line!(out, "{}{}{}",
            box_chars::T_RIGHT,
            horizontal_line(width - 2),
            box_chars::T_LEFT
        );

        let balance_str = match data.wallet_balance {
            Some(bal) => format!("{:.4} OURO", bal as f64 / 100_000_000.0),
            None => "Loading...".to_string(),
        };

        dash_line!(out, "{} {}WALLET:{} {}...{} {} Balance: {}",
            box_chars::VERTICAL,
            colors::BOLD,
            colors::RESET,
            &addr[..8],
            &addr[addr.len() - 6..],
            " ".repeat(3),
            balance_str
        );
    }

    // Alerts section
    if !data.alerts.is_empty() {
        dash_line!(out, "{}{}{}",
            box_chars::T_RIGHT,
            horizontal_line(width - 2),
            box_chars::T_LEFT
        );

        dash_line!(out, "{} {}ALERTS{}{}",
            box_chars::VERTICAL,
            colors::BOLD,
            colors::RESET,
            " ".repeat(width - 10)
        );

        for alert in &data.alerts {
            dash_line!(out, "{} {}{} {}{}",
                box_chars::VERTICAL,
                alert.level.color(),
                alert.level.symbol(),
                alert.message,
                colors::RESET
            );
        }
    }

    // Footer
    dash_line!(out, "{}{}{}",
        box_chars::BOTTOM_LEFT,
        horizontal_line(width - 2),
        box_chars::BOTTOM_RIGHT
    );

    dash_line!(out, "{}v{} | Press Ctrl+C to exit | ouro --help for commands{}",
        colors::DIM,
        data.version,
        colors::RESET
    );
}

/// Print compact status line (for embedding in other output)
pub fn print_status_line(data: &DashboardData) {
    println!(
        "{}{} {}{} | View: {} | Height: {}/{} ({:.1}%) | Peers: {} | TPS: {:.1}",
        data.status.color(),
        data.status.symbol(),
        data.status.label(),
        colors::RESET,
        data.view,
        data.local_height,
        data.network_tip,
        data.sync_percent,
        data.peer_count,
        data.tps_1m
    );
}

/// Print peers list
pub fn print_peers(peers: &[PeerInfo]) {
    println!(
        "\n{}{}CONNECTED PEERS ({}){}",
        colors::BOLD,
        colors::CYAN,
        peers.len(),
        colors::RESET
    );
    println!("{}", horizontal_line(60));
    println!("{:<20} {:<25} {:<10} {:>10}", "ID", "ADDRESS", "ROLE", "LATENCY");
    println!("{}", horizontal_line(70));

    for peer in peers {
        let latency_color = if peer.latency_ms < 50 {
            colors::GREEN
        } else if peer.latency_ms < 200 {
            colors::YELLOW
        } else {
            colors::RED
        };

        println!(
            "{:<20} {:<25} {:<10} {}{}ms{}",
            peer.id,
            peer.addr,
            peer.role,
            latency_color,
            peer.latency_ms,
            colors::RESET
        );
    }
    println!();
}

/// Print consensus info
pub fn print_consensus(data: &DashboardData) {
    println!(
        "\n{}{}CONSENSUS STATUS{}",
        colors::BOLD,
        colors::CYAN,
        colors::RESET
    );
    println!("{}", horizontal_line(50));
    println!("View:           {}", data.view);
    println!("Leader:         {}", data.leader);
    println!("Role:           {}", data.role);
    println!(
        "Last Block:     {} (height {})",
        data.last_block_time, data.last_block_height
    );
    println!("Highest QC:     {}", data.highest_qc);
    println!();
}

/// Print mempool info
pub fn print_mempool(data: &DashboardData) {
    println!(
        "\n{}{}MEMPOOL STATUS{}",
        colors::BOLD,
        colors::CYAN,
        colors::RESET
    );
    println!("{}", horizontal_line(50));
    println!("Transactions:   {}", data.mempool_tx_count);
    println!("Avg Age:        {:.1}s", data.mempool_avg_age_secs);
    println!("TPS (1m):       {:.2}/s", data.tps_1m);
    println!("TPS (5m):       {:.2}/s", data.tps_5m);
    println!();
}

/// Print resources info
pub fn print_resources(data: &DashboardData) {
    println!(
        "\n{}{}RESOURCE USAGE{}",
        colors::BOLD,
        colors::CYAN,
        colors::RESET
    );
    println!("{}", horizontal_line(50));

    let cpu_bar = progress_bar(data.cpu_percent, 20);
    let mem_percent = (data.mem_mb as f64 / 8192.0) * 100.0; // Assume 8GB total
    let mem_bar = progress_bar(mem_percent.min(100.0), 20);
    let disk_percent = if data.disk_total_gb > 0.0 {
        (data.disk_used_gb / data.disk_total_gb) * 100.0
    } else {
        0.0
    };
    let disk_bar = progress_bar(disk_percent, 20);

    println!("CPU:    {} {:.1}%", cpu_bar, data.cpu_percent);
    println!("Memory: {} {} MB", mem_bar, data.mem_mb);
    println!(
        "Disk:   {} {:.1}/{:.1} GB",
        disk_bar, data.disk_used_gb, data.disk_total_gb
    );
    println!(
        "Network: ↓{:.1} KB/s  ↑{:.1} KB/s",
        data.net_in_kbps, data.net_out_kbps
    );
    println!();
}

/// Print help for dashboard commands
pub fn print_help() {
    println!(
        "\n{}{}OUROBOROS CLI COMMANDS{}",
        colors::BOLD,
        colors::CYAN,
        colors::RESET
    );
    println!("{}", horizontal_line(60));
    println!();
    println!("{}Node Control:{}", colors::BOLD, colors::RESET);
    println!("  ouro start              Start the node");
    println!("  ouro stop               Stop the node");
    println!("  ouro restart            Restart the node");
    println!("  ouro join --peer <addr> Join network via peer");
    println!();
    println!("{}Monitoring:{}", colors::BOLD, colors::RESET);
    println!("  ouro status             Live-updating dashboard");
    println!("  ouro status --once      Print status and exit");
    println!("  ouro roles              Compare node tiers & roles");
    println!("  ouro peers              List connected peers");
    println!("  ouro consensus          Show consensus status");
    println!("  ouro mempool            Show mempool status");
    println!("  ouro resources          Show resource usage");
    println!();
    println!("{}Logs & Diagnostics:{}", colors::BOLD, colors::RESET);
    println!("  ouro logs               Tail recent logs");
    println!("  ouro logs --export      Export logs to file");
    println!("  ouro diagnose           Run diagnostic checks");
    println!();
    println!("{}Wallet:{}", colors::BOLD, colors::RESET);
    println!("  ouro wallet             Show wallet status");
    println!("  ouro wallet link        Link wallet address");
    println!("  ouro wallet unlink      Unlink wallet");
    println!();
    println!("{}Admin:{}", colors::BOLD, colors::RESET);
    println!("  ouro resync             Resync from network");
    println!("  ouro backup             Backup database");
    println!("  ouro export-logs        Export full log bundle");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(0), "0m");
        assert_eq!(format_uptime(60), "1m");
        assert_eq!(format_uptime(3600), "1h 0m");
        assert_eq!(format_uptime(86400), "1d 0h 0m");
        assert_eq!(format_uptime(90061), "1d 1h 1m");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    #[test]
    fn test_progress_bar() {
        let bar = progress_bar(50.0, 10);
        assert!(bar.contains("█"));
        assert!(bar.contains("░"));
    }
}
