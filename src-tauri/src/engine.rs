//! Thin wrapper around the librqbit session.
//!
//! Everything the Tauri commands need goes through `Engine`, which owns the
//! session, the persisted config, and a side table mapping torrent id ->
//! output directory (librqbit does not expose that publicly).

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ManagedTorrent, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStatsState,
};
use parking_lot::RwLock;

use crate::types::{AddResult, Config, FileView, SessionView, Snapshot, TorrentView};

/// Poll interval for the UI refresh loop.
pub const TICK_MS: u64 = 700;

/// Event name the frontend listens on.
pub const TICK_EVENT: &str = "termtorrent://tick";

pub struct Engine {
    session: Arc<Session>,
    config: RwLock<Config>,
    config_path: PathBuf,
    output_dirs_path: PathBuf,
    /// torrent id -> output directory it was added with.
    output_dirs: RwLock<HashMap<usize, String>>,
}

impl Engine {
    /// Boot the torrent session. `config_dir` holds `config.json` and the
    /// librqbit session state used to resume torrents across restarts.
    /// `default_download_dir` is resolved by the caller because the right
    /// answer differs per platform (app-private storage on Android).
    pub async fn start(config_dir: PathBuf, default_download_dir: PathBuf) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("cannot create config dir {}", config_dir.display()))?;

        let config_path = config_dir.join("config.json");
        let output_dirs_path = config_dir.join("output_dirs.json");
        let mut config =
            load_config(&config_path).unwrap_or_else(|| default_config(&default_download_dir));
        config.download_dir = expand_tilde(&config.download_dir);
        ensure_dir(&config.download_dir)?;

        let opts = SessionOptions {
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(config_dir.join("session")),
            }),
            fastresume: true,
            // Without a range librqbit never opens a TCP listener, so no peer
            // can connect inbound. Same default range rqbit's CLI uses.
            listen_port_range: Some(4240..4260),
            enable_upnp_port_forwarding: true,
            ratelimits: librqbit::limits::LimitsConfig {
                download_bps: config.max_download_bps.and_then(NonZeroU32::new),
                upload_bps: config.max_upload_bps.and_then(NonZeroU32::new),
            },
            ..Default::default()
        };

        let session = Session::new_with_opts(PathBuf::from(&config.download_dir), opts)
            .await
            .context("failed to start torrent session")?;

        let mut output_dirs = load_output_dirs(&output_dirs_path);
        let active: std::collections::HashSet<usize> =
            session.with_torrents(|it| it.map(|(id, _)| id).collect());
        output_dirs.retain(|id, _| active.contains(id));

        Ok(Arc::new(Self {
            session,
            config: RwLock::new(config),
            config_path,
            output_dirs_path,
            output_dirs: RwLock::new(output_dirs),
        }))
    }

    pub fn config(&self) -> Config {
        self.config.read().clone()
    }

    /// Replace the config, apply rate limits, and persist to disk.
    pub fn set_config(&self, mut next: Config) -> Result<Config> {
        next.download_dir = expand_tilde(&next.download_dir);
        ensure_dir(&next.download_dir)?;

        self.session
            .ratelimits
            .set_download_bps(next.max_download_bps.and_then(NonZeroU32::new));
        self.session
            .ratelimits
            .set_upload_bps(next.max_upload_bps.and_then(NonZeroU32::new));

        let stored = next.clone();
        *self.config.write() = next;
        save_config(&self.config_path, &stored)?;
        Ok(stored)
    }

    /// Add by magnet URI, http(s) URL to a .torrent, or local file path.
    pub async fn add_uri(&self, uri: &str, paused: bool) -> Result<AddResult> {
        let uri = uri.trim();
        anyhow::ensure!(!uri.is_empty(), "empty torrent source");

        let add = if uri.starts_with("magnet:")
            || uri.starts_with("http://")
            || uri.starts_with("https://")
        {
            AddTorrent::from_url(uri.to_string())
        } else {
            let path = expand_tilde(uri);
            let bytes =
                std::fs::read(&path).with_context(|| format!("cannot read torrent file {path}"))?;
            AddTorrent::from_bytes(bytes)
        };

        self.add_inner(add, paused).await
    }

    /// Add from raw `.torrent` bytes (used by the file picker / drag & drop).
    pub async fn add_bytes(&self, bytes: Vec<u8>, paused: bool) -> Result<AddResult> {
        anyhow::ensure!(!bytes.is_empty(), "empty torrent file");
        self.add_inner(AddTorrent::from_bytes(bytes), paused).await
    }

    async fn add_inner(&self, add: AddTorrent<'_>, paused: bool) -> Result<AddResult> {
        let output_dir = self.config.read().download_dir.clone();
        let opts = AddTorrentOptions {
            paused,
            overwrite: true,
            output_folder: Some(output_dir.clone()),
            ..Default::default()
        };

        let response = self.session.add_torrent(add, Some(opts)).await?;
        let (id, handle, already_present) = match response {
            AddTorrentResponse::Added(id, handle) => (id, handle, false),
            AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle, true),
            AddTorrentResponse::ListOnly(_) => anyhow::bail!("unexpected list-only response"),
        };

        {
            let mut dirs = self.output_dirs.write();
            dirs.insert(id, output_dir);
            let _ = save_output_dirs(&self.output_dirs_path, &dirs);
        }

        Ok(AddResult {
            id,
            name: handle
                .name()
                .unwrap_or_else(|| short_hash(&handle.info_hash().as_string())),
            already_present,
        })
    }

    pub async fn pause(&self, id: usize) -> Result<()> {
        let handle = self.handle(id)?;
        self.session.pause(&handle).await
    }

    pub async fn resume(&self, id: usize) -> Result<()> {
        let handle = self.handle(id)?;
        self.session.unpause(&handle).await
    }

    pub async fn remove(&self, id: usize, delete_files: bool) -> Result<()> {
        self.session
            .delete(TorrentIdOrHash::Id(id), delete_files)
            .await?;
        {
            let mut dirs = self.output_dirs.write();
            dirs.remove(&id);
            let _ = save_output_dirs(&self.output_dirs_path, &dirs);
        }
        Ok(())
    }

    /// Pause every torrent that is not already paused.
    pub async fn pause_all(&self) -> Result<usize> {
        let ids = self.ids_where(|paused| !paused);
        let mut n = 0;
        for id in ids {
            if self.pause(id).await.is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }

    /// Resume every paused torrent.
    pub async fn resume_all(&self) -> Result<usize> {
        let ids = self.ids_where(|paused| paused);
        let mut n = 0;
        for id in ids {
            if self.resume(id).await.is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }

    fn ids_where(&self, want_paused: impl Fn(bool) -> bool) -> Vec<usize> {
        self.session.with_torrents(|it| {
            it.filter(|(_, h)| want_paused(h.is_paused()))
                .map(|(id, _)| id)
                .collect()
        })
    }

    fn handle(&self, id: usize) -> Result<Arc<ManagedTorrent>> {
        self.session
            .get(TorrentIdOrHash::Id(id))
            .with_context(|| format!("no torrent with id {id}"))
    }

    /// Per-file breakdown for the detail pane.
    pub fn files(&self, id: usize) -> Result<Vec<FileView>> {
        let handle = self.handle(id)?;
        let stats = handle.stats();
        let only = handle.only_files();

        handle.with_metadata(|meta| {
            meta.file_infos
                .iter()
                .enumerate()
                .map(|(index, info)| FileView {
                    index,
                    name: info.relative_filename.to_string_lossy().into_owned(),
                    length: info.len,
                    downloaded: stats.file_progress.get(index).copied().unwrap_or(0),
                    included: only.as_ref().is_none_or(|list| list.contains(&index)),
                })
                .collect()
        })
    }

    /// Where a torrent's data lives on disk.
    pub fn output_dir(&self, id: usize) -> String {
        self.output_dirs
            .read()
            .get(&id)
            .cloned()
            .unwrap_or_else(|| self.config.read().download_dir.clone())
    }

    /// Build one full UI frame.
    pub fn snapshot(&self) -> Snapshot {
        let default_dir = self.config.read().download_dir.clone();
        let dirs = self.output_dirs.read();

        let torrents: Vec<TorrentView> = self.session.with_torrents(|it| {
            it.map(|(id, handle)| {
                let stats = handle.stats();
                let live = stats.live.as_ref();
                let (down_bps, up_bps) = live
                    .map(|l| {
                        (
                            mib_to_bps(l.download_speed.mbps),
                            mib_to_bps(l.upload_speed.mbps),
                        )
                    })
                    .unwrap_or((0, 0));
                let peers = live.map(|l| &l.snapshot.peer_stats);

                let progress = if stats.total_bytes == 0 {
                    0.0
                } else {
                    (stats.progress_bytes as f64 / stats.total_bytes as f64).clamp(0.0, 1.0)
                };

                let file_count = handle.with_metadata(|m| m.file_infos.len()).unwrap_or(0);

                TorrentView {
                    id,
                    name: handle
                        .name()
                        .unwrap_or_else(|| short_hash(&handle.info_hash().as_string())),
                    info_hash: handle.info_hash().as_string(),
                    state: state_label(&stats.state, stats.finished, stats.error.is_some()),
                    error: stats.error.clone(),
                    progress,
                    progress_bytes: stats.progress_bytes,
                    total_bytes: stats.total_bytes,
                    uploaded_bytes: stats.uploaded_bytes,
                    download_bps: down_bps,
                    upload_bps: up_bps,
                    peers_live: peers.map(|p| p.live).unwrap_or(0),
                    peers_queued: peers.map(|p| p.queued).unwrap_or(0),
                    peers_seen: peers.map(|p| p.seen).unwrap_or(0),
                    eta_secs: eta_secs(stats.total_bytes, stats.progress_bytes, down_bps),
                    output_dir: dirs
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| default_dir.clone()),
                    file_count,
                }
            })
            .collect()
        });

        let session = SessionView {
            torrent_count: torrents.len(),
            active_count: torrents
                .iter()
                .filter(|t| matches!(t.state, "downloading" | "seeding" | "checking"))
                .count(),
            download_bps: torrents.iter().map(|t| t.download_bps).sum(),
            upload_bps: torrents.iter().map(|t| t.upload_bps).sum(),
            peers_live: torrents.iter().map(|t| t.peers_live).sum(),
            listen_port: self.session.tcp_listen_port(),
            dht_enabled: self.session.get_dht().is_some(),
        };

        Snapshot { torrents, session }
    }

    /// Graceful shutdown so librqbit flushes its persistence files.
    pub async fn stop(&self) {
        self.session.stop().await;
    }
}

/// librqbit's `time_remaining` keeps its inner `Duration` private, so derive the
/// ETA from the bytes left and the current download rate instead.
fn eta_secs(total: u64, done: u64, down_bps: u64) -> Option<u64> {
    let remaining = total.checked_sub(done)?;
    if remaining == 0 || down_bps == 0 {
        return None;
    }
    Some(remaining / down_bps)
}

fn state_label(state: &TorrentStatsState, finished: bool, has_error: bool) -> &'static str {
    if has_error {
        return "error";
    }
    match state {
        TorrentStatsState::Initializing => "checking",
        TorrentStatsState::Live if finished => "seeding",
        TorrentStatsState::Live => "downloading",
        TorrentStatsState::Paused if finished => "done",
        TorrentStatsState::Paused => "paused",
        TorrentStatsState::Error => "error",
    }
}

fn mib_to_bps(mib_per_sec: f64) -> u64 {
    if !mib_per_sec.is_finite() || mib_per_sec <= 0.0 {
        return 0;
    }
    (mib_per_sec * 1024.0 * 1024.0).round() as u64
}

fn short_hash(hash: &str) -> String {
    format!("torrent-{}", &hash[..hash.len().min(8)])
}

fn ensure_dir(dir: &str) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create download dir {dir}"))
}

pub fn default_config(download_dir: &Path) -> Config {
    Config {
        download_dir: download_dir.to_string_lossy().into_owned(),
        max_download_bps: None,
        max_upload_bps: None,
        theme: "phosphor".into(),
        crt: true,
    }
}

fn expand_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let rest = path.strip_prefix('~').unwrap_or("");
            let rest = rest.trim_start_matches(['/', '\\']);
            let joined = if rest.is_empty() {
                PathBuf::from(home)
            } else {
                PathBuf::from(home).join(rest)
            };
            return joined.to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

fn load_config(path: &Path) -> Option<Config> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_config(path: &Path, config: &Config) -> Result<()> {
    let raw = serde_json::to_string_pretty(config)?;
    std::fs::write(path, raw).with_context(|| format!("cannot write {}", path.display()))
}

fn load_output_dirs(path: &Path) -> HashMap<usize, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_output_dirs(path: &Path, dirs: &HashMap<usize, String>) -> Result<()> {
    let raw = serde_json::to_string_pretty(dirs)?;
    std::fs::write(path, raw).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_secs_handles_edge_cases() {
        assert_eq!(eta_secs(1000, 1000, 100), None);
        assert_eq!(eta_secs(1000, 500, 0), None);
        assert_eq!(eta_secs(500, 1000, 100), None);
        assert_eq!(eta_secs(1000, 250, 250), Some(3));
    }

    #[test]
    fn state_label_maps_all_variants() {
        assert_eq!(state_label(&TorrentStatsState::Live, false, true), "error");
        assert_eq!(
            state_label(&TorrentStatsState::Initializing, false, false),
            "checking"
        );
        assert_eq!(
            state_label(&TorrentStatsState::Live, true, false),
            "seeding"
        );
        assert_eq!(
            state_label(&TorrentStatsState::Live, false, false),
            "downloading"
        );
        assert_eq!(
            state_label(&TorrentStatsState::Paused, true, false),
            "done"
        );
        assert_eq!(
            state_label(&TorrentStatsState::Paused, false, false),
            "paused"
        );
        assert_eq!(
            state_label(&TorrentStatsState::Error, false, false),
            "error"
        );
    }

    #[test]
    fn mib_to_bps_rejects_invalid_and_converts() {
        assert_eq!(mib_to_bps(0.0), 0);
        assert_eq!(mib_to_bps(-1.5), 0);
        assert_eq!(mib_to_bps(f64::NAN), 0);
        assert_eq!(mib_to_bps(f64::INFINITY), 0);
        assert_eq!(mib_to_bps(1.0), 1024 * 1024);
    }

    #[test]
    fn short_hash_truncates_safely() {
        assert_eq!(short_hash("abcdef123456"), "torrent-abcdef12");
        assert_eq!(short_hash("abc"), "torrent-abc");
    }

    #[test]
    fn config_deserializes_without_crt_field() {
        let json = r#"{"downloadDir":"/tmp/dl","maxDownloadBps":null,"maxUploadBps":1024,"theme":"amber"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert!(cfg.crt);
        assert_eq!(cfg.theme, "amber");
        assert_eq!(cfg.max_upload_bps, Some(1024));
    }

    #[test]
    fn expand_tilde_leaves_non_tilde_paths_intact() {
        assert_eq!(expand_tilde("/tmp/a.torrent"), "/tmp/a.torrent");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
        let expanded = expand_tilde("~/Downloads");
        assert!(!expanded.starts_with("~/"));
    }
}
