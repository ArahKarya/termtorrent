// Thin wrapper over the Tauri IPC surface.
// `withGlobalTauri` is enabled, so no bundler is needed.

const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;

if (!invoke) {
  document.body?.classList.add("no-ipc");
  console.error("Tauri IPC unavailable — running outside the app shell?");
}

const call = (cmd, args) => {
  if (!invoke) return Promise.reject(new Error("IPC unavailable"));
  return invoke(cmd, args);
};

export const listen = (event, handler) => tauri.event.listen(event, handler);

export const api = {
  getConfig: () => call("get_config"),
  setConfig: (config) => call("set_config", { config }),
  snapshot: () => call("snapshot"),
  add: (uri, paused = false) => call("add_torrent", { uri, paused }),
  addBytes: (bytes, paused = false) => call("add_torrent_bytes", { bytes, paused }),
  pause: (id) => call("pause_torrent", { id }),
  resume: (id) => call("resume_torrent", { id }),
  remove: (id, deleteFiles) => call("remove_torrent", { id, deleteFiles }),
  pauseAll: () => call("pause_all"),
  resumeAll: () => call("resume_all"),
  files: (id) => call("torrent_files", { id }),
  outputDir: (id) => call("torrent_output_dir", { id }),
};

/** Native open-file dialog (tauri-plugin-dialog). */
export const pickTorrentFile = () =>
  call("plugin:dialog|open", {
    options: {
      title: "Select a .torrent file",
      multiple: false,
      directory: false,
      filters: [{ name: "Torrent", extensions: ["torrent"] }],
    },
  });

/** Native folder picker, used by `/set dir`. */
export const pickDirectory = (defaultPath) =>
  call("plugin:dialog|open", {
    options: { title: "Choose download folder", directory: true, defaultPath },
  });

/** Reveal a path in Finder/Explorer (tauri-plugin-opener). */
export const revealInDir = (path) => call("plugin:opener|reveal_item_in_dir", { paths: [path] });

/** Open a path with the OS default handler. */
export const openPath = (path) => call("plugin:opener|open_path", { path, with: null });
