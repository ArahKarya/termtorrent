//! Serializable views handed to the frontend.
//!
//! Nothing here holds a reference into the torrent session: every struct is a
//! plain snapshot so the UI thread never blocks on engine locks.

use serde::{Deserialize, Serialize};

/// Persisted user settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Where completed/partial files land.
    pub download_dir: String,
    /// Global download cap in bytes/sec. `None` = unlimited.
    pub max_download_bps: Option<u32>,
    /// Global upload cap in bytes/sec. `None` = unlimited.
    pub max_upload_bps: Option<u32>,
    /// Terminal colour scheme id used by the UI.
    pub theme: String,
    /// Whether the CRT scanline overlay is enabled.
    #[serde(default = "default_crt")]
    pub crt: bool,
}

fn default_crt() -> bool {
    true
}

/// One file inside a torrent.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub index: usize,
    pub name: String,
    pub length: u64,
    pub downloaded: u64,
    pub included: bool,
}

/// Snapshot of a single torrent, refreshed on every tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentView {
    pub id: usize,
    pub name: String,
    pub info_hash: String,
    /// One of: checking | downloading | seeding | paused | done | error
    pub state: &'static str,
    pub error: Option<String>,
    pub progress: f64,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_bps: u64,
    pub upload_bps: u64,
    pub peers_live: usize,
    pub peers_queued: usize,
    pub peers_seen: usize,
    pub eta_secs: Option<u64>,
    pub output_dir: String,
    pub file_count: usize,
}

/// Aggregate numbers for the status bar.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub torrent_count: usize,
    pub active_count: usize,
    pub download_bps: u64,
    pub upload_bps: u64,
    pub peers_live: usize,
    pub listen_port: Option<u16>,
    pub dht_enabled: bool,
}

/// Everything the UI needs for one repaint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub torrents: Vec<TorrentView>,
    pub session: SessionView,
}

/// Result of adding a torrent, so the UI can echo a useful line.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub id: usize,
    pub name: String,
    pub already_present: bool,
}
