//! Headless terminal CLI for termtorrent — no GUI, no WebView, no GTK/WebKitGTK.
//!
//! Build on Linux without GUI dependencies:
//!   cargo build --release --no-default-features --bin termtorrent-cli

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use termtorrent_lib::engine::{Engine, TICK_MS};
use termtorrent_lib::types::TorrentView;
use tokio::sync::mpsc;

const APP_ID: &str = "dev.arahkarya.termtorrent";
const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
const EIGHTHS: &[char] = &[' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

struct CliState {
    selected_id: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                // librqbit logs a WARN for every duplicate chunk request from a peer
                // ("we already requested ChunkInfo ..."). It is harmless (the duplicate
                // is dropped and every piece is hash-verified) but floods the REPL, so
                // only real errors from that module are shown by default.
                .unwrap_or_else(|_| {
                    "termtorrent=warn,librqbit=warn,librqbit::torrent_state::live=error".into()
                }),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_cli_usage();
        return Ok(());
    }

    let config_dir = resolve_config_dir();
    let download_dir = resolve_download_dir();
    let engine = Engine::start(config_dir, download_dir).await?;
    let mut state = CliState { selected_id: None };

    if !args.is_empty() {
        let wait = args.iter().any(|a| a == "--wait");
        let filtered: Vec<String> = args.into_iter().filter(|a| a != "--wait").collect();
        let line = filtered.join(" ");
        let res = dispatch(&engine, &mut state, &line).await;
        if let Err(e) = res {
            eprintln!("error: {e:#}");
            engine.stop().await;
            std::process::exit(1);
        }
        if wait {
            wait_loop(&engine, state.selected_id).await;
        }
        engine.stop().await;
        return Ok(());
    }

    run_repl(&engine, &mut state).await;
    engine.stop().await;
    Ok(())
}

async fn run_repl(engine: &Arc<Engine>, state: &mut CliState) {
    let snap = engine.snapshot();
    let cfg = engine.config();
    println!(
        "termtorrent v{} (headless CLI) — port {} | dht {} | dir {}",
        env!("CARGO_PKG_VERSION"),
        snap.session
            .listen_port
            .map_or_else(|| "—".into(), |p| p.to_string()),
        if snap.session.dht_enabled { "on" } else { "off" },
        cfg.download_dir
    );
    println!("type /help for commands, or paste a magnet link / .torrent path");

    let (tx, mut rx) = mpsc::channel::<Option<String>>(16);
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        loop {
            match lines.next() {
                Some(Ok(line)) => {
                    if tx.blocking_send(Some(line)).is_err() {
                        break;
                    }
                }
                _ => {
                    let _ = tx.blocking_send(None);
                    break;
                }
            }
        }
    });

    loop {
        print!("$ ");
        let _ = io::stdout().flush();

        let line = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nshutting down…");
                break;
            }
            msg = rx.recv() => match msg {
                Some(Some(l)) => l,
                _ => {
                    println!();
                    break;
                }
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cmd_name = trimmed.strip_prefix('/').unwrap_or(trimmed);
        if matches!(cmd_name, "quit" | "exit" | "q") {
            break;
        }
        if matches!(cmd_name, "watch" | "w") {
            watch_loop(engine, &mut rx).await;
            continue;
        }

        if let Err(e) = dispatch(engine, state, trimmed).await {
            eprintln!("error: {e:#}");
        }
    }
}

async fn watch_loop(engine: &Engine, rx: &mut mpsc::Receiver<Option<String>>) {
    println!("watching session (press Enter or Ctrl+C to stop)…");
    let mut interval = tokio::time::interval(Duration::from_millis(TICK_MS));
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            msg = rx.recv() => {
                if msg.is_none() || matches!(msg, Some(None)) {
                    break;
                }
                break;
            }
            _ = interval.tick() => {
                let snap = engine.snapshot();
                print!("\x1b[2J\x1b[H");
                println!(
                    "SESS {} | ▼ {} | ▲ {} | peers {} | port {}",
                    snap.session.torrent_count,
                    fmt_speed(snap.session.download_bps),
                    fmt_speed(snap.session.upload_bps),
                    snap.session.peers_live,
                    snap.session.listen_port.map_or_else(|| "—".into(), |p| p.to_string()),
                );
                print_torrents(&snap.torrents);
                println!("(press Enter to return to prompt)");
                let _ = io::stdout().flush();
            }
        }
    }
}

async fn wait_loop(engine: &Engine, target_id: Option<usize>) {
    let mut interval = tokio::time::interval(Duration::from_millis(TICK_MS));
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\ninterrupted — flushing session…");
                break;
            }
            _ = interval.tick() => {
                let snap = engine.snapshot();
                let t = target_id
                    .and_then(|id| snap.torrents.iter().find(|t| t.id == id))
                    .or_else(|| snap.torrents.last());
                let Some(t) = t else { break };
                print!(
                    "\r[{}] {} {} {} ▼{} eta {}   ",
                    t.id,
                    progress_bar(t.progress, 20),
                    fmt_pct(t.progress),
                    t.state,
                    fmt_speed(t.download_bps),
                    fmt_eta(t.eta_secs),
                );
                let _ = io::stdout().flush();
                if matches!(t.state, "seeding" | "done" | "error") {
                    println!();
                    if let Some(err) = &t.error {
                        eprintln!("error: {err}");
                    }
                    break;
                }
            }
        }
    }
}

async fn dispatch(engine: &Engine, state: &mut CliState, raw: &str) -> Result<()> {
    let line = raw.trim();
    if line.is_empty() {
        return Ok(());
    }

    if let Some(rest) = line.strip_prefix('/') {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        let Some((&head, args)) = parts.split_first() else {
            return Ok(());
        };
        if is_known_command(head) {
            return run_command(engine, state, head, args).await;
        }
        if looks_like_source(line) {
            return add_source(engine, state, line).await;
        }
        bail!("unknown command: /{head} — type /help");
    }

    if looks_like_source(line) {
        return add_source(engine, state, line).await;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    let Some((&head, args)) = parts.split_first() else {
        return Ok(());
    };
    if !is_known_command(head) {
        bail!("unknown command: {head} — type /help");
    }
    run_command(engine, state, head, args).await
}

fn is_known_command(name: &str) -> bool {
    matches!(
        name,
        "help"
            | "h"
            | "?"
            | "add"
            | "ls"
            | "list"
            | "pause"
            | "p"
            | "resume"
            | "r"
            | "start"
            | "rm"
            | "del"
            | "remove"
            | "files"
            | "f"
            | "info"
            | "i"
            | "sel"
            | "select"
            | "config"
            | "cfg"
            | "set"
    )
}

async fn run_command(
    engine: &Engine,
    state: &mut CliState,
    head: &str,
    args: &[&str],
) -> Result<()> {
    match head {
        "help" | "h" | "?" => print_repl_help(),
        "add" => {
            let uri = args.join(" ");
            if uri.trim().is_empty() {
                bail!("usage: /add <magnet|url|path>");
            }
            add_source(engine, state, &uri).await?;
        }
        "ls" | "list" => {
            let snap = engine.snapshot();
            print_torrents(&snap.torrents);
        }
        "pause" | "p" => {
            if args.first() == Some(&"all") {
                let n = engine.pause_all().await?;
                println!("paused {n} torrent(s)");
            } else {
                let id = resolve_id(engine, state, args.first().copied())?;
                engine.pause(id).await?;
                println!("paused [{id}]");
            }
        }
        "resume" | "r" | "start" => {
            if args.first() == Some(&"all") {
                let n = engine.resume_all().await?;
                println!("resumed {n} torrent(s)");
            } else {
                let id = resolve_id(engine, state, args.first().copied())?;
                engine.resume(id).await?;
                println!("resumed [{id}]");
            }
        }
        "rm" | "del" | "remove" => {
            let delete_files = args.contains(&"--files");
            let target = args.iter().copied().find(|a| !a.starts_with("--"));
            let id = resolve_id(engine, state, target)?;
            engine.remove(id, delete_files).await?;
            if state.selected_id == Some(id) {
                state.selected_id = None;
            }
            println!(
                "removed [{id}]{}",
                if delete_files { " (data deleted)" } else { "" }
            );
        }
        "files" | "f" => {
            let id = resolve_id(engine, state, args.first().copied())?;
            let files = engine.files(id)?;
            if files.is_empty() {
                println!("metadata not resolved yet");
            } else {
                for f in files {
                    let pct = if f.length > 0 {
                        f.downloaded as f64 / f.length as f64
                    } else {
                        0.0
                    };
                    println!(
                        "  {}  {:>9}  {} {}",
                        fmt_pct(pct),
                        fmt_bytes(f.length),
                        if f.included { ' ' } else { '-' },
                        f.name
                    );
                }
            }
        }
        "info" | "i" => {
            let id = resolve_id(engine, state, args.first().copied())?;
            let snap = engine.snapshot();
            let t = snap
                .torrents
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| anyhow::anyhow!("no torrent with id {id}"))?;
            println!("name    {}", t.name);
            println!("hash    {}", t.info_hash);
            println!(
                "state   {}{}",
                t.state,
                t.error
                    .as_deref()
                    .map_or_else(String::new, |e| format!(" — {e}"))
            );
            println!(
                "size    {} / {} ({})",
                fmt_bytes(t.progress_bytes),
                fmt_bytes(t.total_bytes),
                fmt_pct(t.progress)
            );
            println!(
                "rate    ▼ {}   ▲ {}   eta {}",
                fmt_speed(t.download_bps),
                fmt_speed(t.upload_bps),
                fmt_eta(t.eta_secs)
            );
            println!(
                "peers   {} live / {} queued / {} seen",
                t.peers_live, t.peers_queued, t.peers_seen
            );
            println!("files   {}", t.file_count);
            println!("dir     {}", t.output_dir);
        }
        "sel" | "select" => {
            let id = resolve_id(engine, state, args.first().copied())?;
            state.selected_id = Some(id);
            println!("selected [{id}]");
        }
        "config" | "cfg" => {
            let c = engine.config();
            println!("dir     {}", c.download_dir);
            println!("down    {}", rate_label(c.max_download_bps));
            println!("up      {}", rate_label(c.max_upload_bps));
            println!("theme   {}", c.theme);
            println!("crt     {}", if c.crt { "on" } else { "off" });
        }
        "set" => {
            let Some(&key) = args.first() else {
                bail!("usage: /set <dir|down|up> <value>");
            };
            let value = args[1..].join(" ");
            let mut cfg = engine.config();
            match key {
                "dir" => {
                    if value.trim().is_empty() {
                        bail!("usage: /set dir <path>");
                    }
                    cfg.download_dir = value.trim().to_string();
                    let saved = engine.set_config(cfg)?;
                    println!("download dir → {}", saved.download_dir);
                }
                "down" => {
                    let bps = parse_rate(value.trim())?;
                    cfg.max_download_bps = bps;
                    engine.set_config(cfg)?;
                    println!("download limit → {}", rate_label(bps));
                }
                "up" => {
                    let bps = parse_rate(value.trim())?;
                    cfg.max_upload_bps = bps;
                    engine.set_config(cfg)?;
                    println!("upload limit → {}", rate_label(bps));
                }
                _ => bail!("usage: /set <dir|down|up> <value>"),
            }
        }
        _ => bail!("unknown command: {head}"),
    }
    Ok(())
}

async fn add_source(engine: &Engine, state: &mut CliState, uri: &str) -> Result<()> {
    let clean = uri.strip_prefix("file://").unwrap_or(uri);
    let res = engine.add_uri(clean, false).await?;
    state.selected_id = Some(res.id);
    if res.already_present {
        println!("[{}] already in session — {}", res.id, res.name);
    } else {
        println!("[{}] added — {}", res.id, res.name);
    }
    Ok(())
}

fn resolve_id(engine: &Engine, state: &CliState, token: Option<&str>) -> Result<usize> {
    let id = match token {
        Some(t) => t
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("not a torrent id: {t}"))?,
        None => state
            .selected_id
            .ok_or_else(|| anyhow::anyhow!("no torrent selected — pass an id or run /sel <id>"))?,
    };
    let exists = engine.snapshot().torrents.iter().any(|t| t.id == id);
    if !exists {
        bail!("no torrent with id {id}");
    }
    Ok(id)
}

fn print_torrents(torrents: &[TorrentView]) {
    if torrents.is_empty() {
        println!("session is empty");
        return;
    }
    for t in torrents {
        println!(
            "[{:>2}] {} {}  {:<11} ▼{:>11}  {:>9}  {}",
            t.id,
            progress_bar(t.progress, 16),
            fmt_pct(t.progress),
            t.state,
            fmt_speed(t.download_bps),
            fmt_bytes(t.total_bytes),
            t.name
        );
    }
}

fn print_cli_usage() {
    println!("termtorrent-cli — headless BitTorrent client");
    println!();
    println!("USAGE:");
    println!("  termtorrent-cli                     Start interactive terminal REPL");
    println!("  termtorrent-cli <magnet|url|path>   Add a torrent and exit");
    println!("  termtorrent-cli add <src> [--wait]  Add a torrent (optionally wait until done)");
    println!("  termtorrent-cli <command> [args]    Run a single command (ls, info, pause, ...)");
    println!();
    println!("ENV:");
    println!("  TERMTORRENT_CONFIG_DIR              Override config/session directory");
}

fn print_repl_help() {
    println!("commands (leading / is optional; bare magnet, URL or path is added directly):");
    println!("  /help                      list commands");
    println!("  /add <magnet|url|path>     add a torrent");
    println!("  /ls                        print the torrent table");
    println!("  /watch                     live-refreshing status view");
    println!("  /pause <id|all>            pause a torrent");
    println!("  /resume <id|all>           resume a torrent");
    println!("  /rm <id> [--files]         remove a torrent (--files deletes data)");
    println!("  /files [id]                list files inside a torrent");
    println!("  /info [id]                 show torrent details");
    println!("  /sel <id>                  select a torrent for id-less commands");
    println!("  /config                    print current settings");
    println!("  /set <dir|down|up> <val>   change a setting");
    println!("  /quit                      flush session and exit");
}

fn looks_like_source(text: &str) -> bool {
    text.starts_with("magnet:")
        || text.starts_with("http://")
        || text.starts_with("https://")
        || text.starts_with('/')
        || text.starts_with("~/")
        || text.starts_with("file://")
        || text.to_ascii_lowercase().ends_with(".torrent")
}

fn parse_rate(token: &str) -> Result<Option<u32>> {
    if token.is_empty() {
        bail!("missing rate (e.g. 2M, 500k, off)");
    }
    let t = token.to_ascii_lowercase();
    if matches!(t.as_str(), "0" | "off" | "none" | "unlimited") {
        return Ok(None);
    }
    let t = t
        .strip_suffix("/s")
        .or_else(|| t.strip_suffix("b/s"))
        .unwrap_or(&t)
        .trim_end_matches('b')
        .trim();

    let (num_str, mult) = match t.chars().last() {
        Some('k') => (&t[..t.len() - 1], 1024_f64),
        Some('m') => (&t[..t.len() - 1], 1024_f64 * 1024.0),
        Some('g') => (&t[..t.len() - 1], 1024_f64 * 1024.0 * 1024.0),
        _ => (t, 1.0),
    };
    let val: f64 = num_str
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("bad rate: {token} (try 2M, 500k, off)"))?;
    if !val.is_finite() || val < 0.0 {
        bail!("bad rate: {token}");
    }
    let bps = (val * mult).round();
    if bps > u32::MAX as f64 {
        bail!("rate too large: {token} (max ~4G)");
    }
    let bps = bps as u32;
    Ok((bps > 0).then_some(bps))
}

fn rate_label(bps: Option<u32>) -> String {
    bps.map_or_else(|| "unlimited".into(), |b| fmt_speed(b as u64))
}

fn fmt_bytes(n: u64) -> String {
    if n == 0 {
        return "0 B".into();
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 || i == 0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

fn fmt_speed(bps: u64) -> String {
    format!("{}/s", fmt_bytes(bps))
}

fn fmt_pct(progress: f64) -> String {
    format!("{:>5.1}%", (progress * 100.0).clamp(0.0, 100.0))
}

fn fmt_eta(secs: Option<u64>) -> String {
    let Some(s) = secs else {
        return "—".into();
    };
    if s >= 86400 {
        format!("{}d{:02}h", s / 86400, (s % 86400) / 3600)
    } else if s >= 3600 {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

fn progress_bar(progress: f64, width: usize) -> String {
    let clamped = progress.clamp(0.0, 1.0);
    let exact = clamped * width as f64;
    let full = (exact.floor() as usize).min(width);
    let mut out = "█".repeat(full);
    if full < width {
        let idx = ((exact - full as f64) * 8.0).round() as usize % 8;
        out.push(EIGHTHS[idx]);
        out.push_str(&"░".repeat(width - full - 1));
    }
    out
}

fn resolve_config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TERMTORRENT_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(APP_ID);
    }
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join(APP_ID);
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join(APP_ID);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join(APP_ID);
    }
    PathBuf::from(".")
}

fn resolve_download_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Downloads").join("termtorrent")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rate_valid_and_overflow() {
        assert_eq!(parse_rate("off").unwrap(), None);
        assert_eq!(parse_rate("0").unwrap(), None);
        assert_eq!(parse_rate("500k").unwrap(), Some(500 * 1024));
        assert_eq!(parse_rate("2M").unwrap(), Some(2 * 1024 * 1024));
        assert!(parse_rate("4G").is_err());
        assert!(parse_rate("abc").is_err());
    }

    #[test]
    fn looks_like_source_detects_uris_and_paths() {
        assert!(looks_like_source("magnet:?xt=urn:btih:123"));
        assert!(looks_like_source("https://example.com/a.torrent"));
        assert!(looks_like_source("~/Downloads/a.torrent"));
        assert!(looks_like_source("ubuntu.TORRENT"));
        assert!(!looks_like_source("ls"));
    }

    #[test]
    fn format_helpers_produce_expected_strings() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(1536), "1.5 KiB");
        assert_eq!(fmt_eta(None), "—");
        assert_eq!(fmt_eta(Some(3661)), "1h01m");
    }
}
