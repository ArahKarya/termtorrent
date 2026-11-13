// App wiring: boot the session view, drive the prompt, keep the UI in sync.

import { api, listen } from "./api.js";
import { lookup } from "./commands.js";
import * as palette from "./palette.js";
import {
  clearLog,
  log,
  onRowClick,
  renderSnapshot,
  setBadge,
  setDownloadDir,
} from "./render.js";

const input = document.getElementById("input");

/**
 * Tag the document with the host OS so the stylesheet can adapt — chiefly the
 * left inset macOS needs for its traffic lights, which no other platform has.
 * Sniffing the UA avoids pulling in the os plugin for one string.
 */
function detectOs() {
  const ua = navigator.userAgent;
  if (/Android/i.test(ua)) return "android";
  if (/Windows/i.test(ua)) return "windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  return "linux";
}

const OS = detectOs();
document.documentElement.dataset.os = OS;

const state = {
  snapshot: { torrents: [], session: {} },
  config: { downloadDir: "", maxDownloadBps: null, maxUploadBps: null, theme: "phosphor", crt: true },
  selectedId: null,
  ready: false,
};

const history = [];
let historyPos = 0;

const ctx = {
  state,
  log,
  addSource,
  applyConfig,
};

/** Anything that is not a command: magnet, http(s) URL, or local .torrent path. */
function looksLikeSource(text) {
  return (
    text.startsWith("magnet:") ||
    text.startsWith("http://") ||
    text.startsWith("https://") ||
    text.startsWith("/") ||
    text.startsWith("~/") ||
    text.startsWith("file://") ||
    /\.torrent$/i.test(text)
  );
}

async function addSource(uri) {
  const clean = uri.startsWith("file://") ? decodeURIComponent(uri.slice(7)) : uri;
  const res = await api.add(clean, false);
  state.selectedId = res.id;
  log(
    res.alreadyPresent
      ? `[${res.id}] already in session — ${res.name}`
      : `[${res.id}] added — ${res.name}`,
    res.alreadyPresent ? "warn" : "ok",
  );
  await refresh();
}

async function applyConfig(next) {
  state.config = await api.setConfig(next);
  setDownloadDir(state.config.downloadDir);
  document.documentElement.dataset.theme = state.config.theme;
  document.documentElement.dataset.crt = state.config.crt ? "on" : "off";
}

async function refresh() {
  state.snapshot = await api.snapshot();
  renderSnapshot(state.snapshot, state.selectedId);
}

/**
 * Slash lines are commands first and file paths second, so `/help` runs the
 * command while `/Users/me/x.torrent` still resolves as a path.
 */
async function dispatch(line) {
  if (line.startsWith("/")) {
    const [head, ...args] = line.slice(1).split(/\s+/);
    const cmd = lookup(head);
    if (cmd) return cmd.run(ctx, args);
    if (looksLikeSource(line)) return addSource(line);
    throw new Error(`unknown command: /${head} — type /help`);
  }

  if (looksLikeSource(line)) return addSource(line);

  const [head, ...args] = line.split(/\s+/);
  const cmd = lookup(head);
  if (!cmd) throw new Error(`unknown command: ${head} — type /help`);
  return cmd.run(ctx, args);
}

async function runLine(raw) {
  const line = raw.trim();
  palette.close();
  if (!line) return;

  log(`$ ${line}`, "echo");
  history.push(line);
  historyPos = history.length;

  try {
    if (!state.ready) throw new Error("session is still starting up — try again in a moment");
    await dispatch(line);
  } catch (e) {
    log(`error: ${e?.message ?? e}`, "err");
  }
}

// ── slash palette ───────────────────────────────────────────────────────────

/** Accept a palette entry: run it outright, or prefill it for its arguments. */
async function accept(match) {
  if (!match) return;
  palette.close();
  if (match.cmd.args === "none") {
    input.value = "";
    await runLine(`/${match.shown}`);
  } else {
    input.value = `/${match.shown} `;
  }
  input.focus();
}

palette.onPick(accept);
input.addEventListener("input", () => palette.update(input.value));

// ── prompt behaviour ────────────────────────────────────────────────────────

input.addEventListener("keydown", async (e) => {
  if (palette.isOpen()) {
    if (e.key === "ArrowUp" || (e.key === "p" && e.ctrlKey)) {
      e.preventDefault();
      return palette.move(-1);
    }
    if (e.key === "ArrowDown" || (e.key === "n" && e.ctrlKey)) {
      e.preventDefault();
      return palette.move(1);
    }
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      return accept(palette.selected());
    }
    if (e.key === "Escape") {
      e.preventDefault();
      return palette.close();
    }
  }

  if (e.key === "Enter") {
    const value = input.value;
    input.value = "";
    await runLine(value);
    return;
  }

  if (e.key === "ArrowUp") {
    e.preventDefault();
    if (historyPos > 0) input.value = history[--historyPos] ?? "";
    return;
  }

  if (e.key === "ArrowDown") {
    e.preventDefault();
    historyPos = Math.min(historyPos + 1, history.length);
    input.value = history[historyPos] ?? "";
    return;
  }

  if (e.key === "Tab") {
    // Empty line: reveal the whole command list, same as typing "/".
    e.preventDefault();
    if (!input.value) {
      input.value = "/";
      palette.update(input.value);
    }
    return;
  }

  if (e.key === "l" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    clearLog();
    return;
  }

  if (e.key === "o" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    await runLine("/open");
  }
});

// Keep focus on the prompt unless the user is selecting console text.
document.addEventListener("mousedown", (e) => {
  if (!e.target.closest(".console") && !e.target.closest(".palette")) {
    setTimeout(() => input.focus(), 0);
  }
});

onRowClick((id) => {
  state.selectedId = state.selectedId === id ? null : id;
  renderSnapshot(state.snapshot, state.selectedId);
});

// ── drag & drop of .torrent files ───────────────────────────────────────────

listen("tauri://drag-over", () => document.body.classList.add("dragover"));
listen("tauri://drag-leave", () => document.body.classList.remove("dragover"));
listen("tauri://drag-drop", async (event) => {
  document.body.classList.remove("dragover");
  const paths = event.payload?.paths ?? [];
  for (const p of paths) {
    try {
      await addSource(p);
    } catch (e) {
      log(`error: ${e?.message ?? e}`, "err");
    }
  }
});

// ── session lifecycle ───────────────────────────────────────────────────────

listen("termtorrent://tick", (event) => {
  state.snapshot = event.payload;
  renderSnapshot(state.snapshot, state.selectedId);
  // `listen` resolves asynchronously, so the one-shot ready event can fire
  // before its handler is attached. Ticks are the fallback trigger.
  if (!state.ready) boot();
});

listen("termtorrent://fatal", (event) => {
  setBadge("session failed", "down");
  log(`fatal: ${event.payload}`, "err");
});

listen("termtorrent://ready", () => boot());

let booting = false;

async function boot() {
  if (state.ready || booting) return;
  booting = true;
  try {
    state.config = await api.getConfig();
    state.ready = true;
    setDownloadDir(state.config.downloadDir);
    document.documentElement.dataset.theme = state.config.theme;
    document.documentElement.dataset.crt = state.config.crt ? "on" : "off";
    setBadge("connected");
    await refresh();
    log("termtorrent ready — paste a magnet link or type /help", "ok");
  } catch {
    // Session not up yet — the ready event or the next tick will retry.
  } finally {
    booting = false;
  }
}

log("booting torrent session…");
boot();
input.focus();
