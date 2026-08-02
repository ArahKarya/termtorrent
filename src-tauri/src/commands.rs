//! Tauri command surface. Every command returns `Result<T, String>` so the
//! frontend can print the error straight into the terminal log.

use std::sync::Arc;

use tauri::State;

use crate::engine::Engine;
use crate::types::{AddResult, Config, FileView, Snapshot};

/// Shared engine handle. `None` until the session finished booting.
pub type EngineState = Arc<parking_lot::RwLock<Option<Arc<Engine>>>>;

fn engine(state: &State<'_, EngineState>) -> Result<Arc<Engine>, String> {
    state
        .read()
        .clone()
        .ok_or_else(|| "torrent session is still starting up".to_string())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_config(state: State<'_, EngineState>) -> Result<Config, String> {
    Ok(engine(&state)?.config())
}

#[tauri::command]
pub fn set_config(state: State<'_, EngineState>, config: Config) -> Result<Config, String> {
    engine(&state)?.set_config(config).map_err(err)
}

#[tauri::command]
pub fn snapshot(state: State<'_, EngineState>) -> Result<Snapshot, String> {
    Ok(engine(&state)?.snapshot())
}

#[tauri::command]
pub async fn add_torrent(
    state: State<'_, EngineState>,
    uri: String,
    paused: bool,
) -> Result<AddResult, String> {
    let engine = engine(&state)?;
    engine.add_uri(&uri, paused).await.map_err(err)
}

#[tauri::command]
pub async fn add_torrent_bytes(
    state: State<'_, EngineState>,
    bytes: Vec<u8>,
    paused: bool,
) -> Result<AddResult, String> {
    let engine = engine(&state)?;
    engine.add_bytes(bytes, paused).await.map_err(err)
}

#[tauri::command]
pub async fn pause_torrent(state: State<'_, EngineState>, id: usize) -> Result<(), String> {
    let engine = engine(&state)?;
    engine.pause(id).await.map_err(err)
}

#[tauri::command]
pub async fn resume_torrent(state: State<'_, EngineState>, id: usize) -> Result<(), String> {
    let engine = engine(&state)?;
    engine.resume(id).await.map_err(err)
}

#[tauri::command]
pub async fn remove_torrent(
    state: State<'_, EngineState>,
    id: usize,
    delete_files: bool,
) -> Result<(), String> {
    let engine = engine(&state)?;
    engine.remove(id, delete_files).await.map_err(err)
}

#[tauri::command]
pub async fn pause_all(state: State<'_, EngineState>) -> Result<usize, String> {
    let engine = engine(&state)?;
    engine.pause_all().await.map_err(err)
}

#[tauri::command]
pub async fn resume_all(state: State<'_, EngineState>) -> Result<usize, String> {
    let engine = engine(&state)?;
    engine.resume_all().await.map_err(err)
}

#[tauri::command]
pub fn torrent_files(state: State<'_, EngineState>, id: usize) -> Result<Vec<FileView>, String> {
    engine(&state)?.files(id).map_err(err)
}

#[tauri::command]
pub fn torrent_output_dir(state: State<'_, EngineState>, id: usize) -> Result<String, String> {
    Ok(engine(&state)?.output_dir(id))
}
