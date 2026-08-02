// Command registry for the prompt. Each command gets `ctx` with the API,
// logging helpers, and mutable UI state.

import { api, openPath, pickDirectory, pickTorrentFile, revealInDir } from "./api.js";
import { fmtBytes, fmtEta, fmtPct, fmtSpeed } from "./format.js";
import { clearLog } from "./render.js";

const THEMES = ["phosphor", "amber", "ice", "mono"];

/** Resolve a user-supplied id token against the current snapshot. */
function resolveId(ctx, token) {
  if (token == null) {
    if (ctx.state.selectedId != null) return ctx.state.selectedId;
    throw new Error("no torrent selected — pass an id or click a row");
  }
  const id = Number(token);
  if (!Number.isInteger(id)) throw new Error(`not a torrent id: ${token}`);
  if (!ctx.state.snapshot.torrents.some((t) => t.id === id)) {
    throw new Error(`no torrent with id ${id}`);
  }
  return id;
}

function torrent(ctx, id) {
  return ctx.state.snapshot.torrents.find((t) => t.id === id);
}

/** Parse "12M", "500k", "0"/"off" into bytes-per-second (null = unlimited). */
function parseRate(token) {
  if (token == null) throw new Error("missing rate (e.g. 2M, 500k, off)");
  const t = token.toLowerCase();
  if (t === "0" || t === "off" || t === "none" || t === "unlimited") return null;
  const m = /^(\d+(?:\.\d+)?)\s*([kmg])?(?:b(?:\/s)?)?$/.exec(t);
  if (!m) throw new Error(`bad rate: ${token} (try 2M, 500k, off)`);
  const mult = { k: 1024, m: 1024 ** 2, g: 1024 ** 3 }[m[2]] ?? 1;
  const bps = Math.round(Number(m[1]) * mult);
  return bps > 0 ? bps : null;
}

const rateLabel = (bps) => (bps ? `${fmtSpeed(bps)}` : "unlimited");

/**
 * File-manager integration exists on desktop only — Android has no folder to
 * hand off to. Fall back to printing the path so the command still says
 * something useful instead of throwing an opaque plugin error.
 */
async function tryOpen(action, path, ctx, verb) {
  try {
    await action(path);
    ctx.log(`${verb} ${path}`, "ok");
  } catch {
    ctx.log(`no file manager available here — path is ${path}`, "warn");
  }
}

export const COMMANDS = [
  {
    name: "help",
    args: "none",
    aliases: ["h", "?"],
    usage: "/help",
    about: "list every command",
    run: (ctx) => {
      ctx.log("commands — a bare magnet link, URL or file path is added directly", "ok");
      for (const c of COMMANDS) {
        const alias = c.aliases.length ? ` (${c.aliases.map((a) => `/${a}`).join(", ")})` : "";
        ctx.log(`  ${c.usage.padEnd(26)} ${c.about}${alias}`);
      }
      ctx.log("  keys: ctrl+o open file · ctrl+l clear · ↑↓ history · tab complete");
    },
  },
  {
    name: "add",
    args: "req",
    aliases: [],
    usage: "/add <magnet|url|path>",
    about: "add a torrent",
    run: async (ctx, args) => {
      const uri = args.join(" ").trim();
      if (!uri) throw new Error("usage: /add <magnet|url|path>");
      await ctx.addSource(uri);
    },
  },
  {
    name: "open",
    args: "none",
    aliases: ["o"],
    usage: "/open",
    about: "pick a .torrent file",
    run: async (ctx) => {
      const picked = await pickTorrentFile();
      if (!picked) return ctx.log("cancelled");
      const path = typeof picked === "string" ? picked : picked.path;
      await ctx.addSource(path);
    },
  },
  {
    name: "ls",
    args: "none",
    aliases: ["list"],
    usage: "/ls",
    about: "print the torrent table",
    run: (ctx) => {
      const { torrents } = ctx.state.snapshot;
      if (!torrents.length) return ctx.log("session is empty");
      for (const t of torrents) {
        ctx.log(
          `[${String(t.id).padStart(2)}] ${fmtPct(t.progress)}  ${t.state.padEnd(11)} ` +
            `▼${fmtSpeed(t.downloadBps).padStart(11)}  ${fmtBytes(t.totalBytes).padStart(9)}  ${t.name}`,
        );
      }
    },
  },
  {
    name: "pause",
    args: "req",
    aliases: ["p"],
    usage: "/pause <id|all>",
    about: "pause a torrent",
    run: async (ctx, [target]) => {
      if (target === "all") {
        const n = await api.pauseAll();
        return ctx.log(`paused ${n} torrent(s)`, "ok");
      }
      const id = resolveId(ctx, target);
      await api.pause(id);
      ctx.log(`paused [${id}] ${torrent(ctx, id)?.name ?? ""}`, "ok");
    },
  },
  {
    name: "resume",
    args: "req",
    aliases: ["r", "start"],
    usage: "/resume <id|all>",
    about: "resume a torrent",
    run: async (ctx, [target]) => {
      if (target === "all") {
        const n = await api.resumeAll();
        return ctx.log(`resumed ${n} torrent(s)`, "ok");
      }
      const id = resolveId(ctx, target);
      await api.resume(id);
      ctx.log(`resumed [${id}] ${torrent(ctx, id)?.name ?? ""}`, "ok");
    },
  },
  {
    name: "rm",
    args: "req",
    aliases: ["del", "remove"],
    usage: "/rm <id> [--files]",
    about: "remove a torrent (--files also deletes data)",
    run: async (ctx, args) => {
      const deleteFiles = args.includes("--files");
      const id = resolveId(ctx, args.find((a) => !a.startsWith("--")));
      const name = torrent(ctx, id)?.name ?? "";
      await api.remove(id, deleteFiles);
      if (ctx.state.selectedId === id) ctx.state.selectedId = null;
      ctx.log(`removed [${id}] ${name}${deleteFiles ? " (data deleted)" : ""}`, "warn");
    },
  },
  {
    name: "files",
    args: "opt",
    aliases: ["f"],
    usage: "/files [id]",
    about: "list files inside a torrent",
    run: async (ctx, [target]) => {
      const id = resolveId(ctx, target);
      const files = await api.files(id);
      if (!files.length) return ctx.log("metadata not resolved yet");
      for (const f of files) {
        const pct = f.length ? fmtPct(f.downloaded / f.length) : fmtPct(0);
        ctx.log(`  ${pct}  ${fmtBytes(f.length).padStart(9)}  ${f.included ? " " : "-"} ${f.name}`);
      }
    },
  },
  {
    name: "info",
    args: "opt",
    aliases: ["i"],
    usage: "/info [id]",
    about: "show torrent details",
    run: (ctx, [target]) => {
      const t = torrent(ctx, resolveId(ctx, target));
      ctx.log(`name    ${t.name}`, "ok");
      ctx.log(`hash    ${t.infoHash}`);
      ctx.log(`state   ${t.state}${t.error ? ` — ${t.error}` : ""}`);
      ctx.log(`size    ${fmtBytes(t.progressBytes)} / ${fmtBytes(t.totalBytes)} (${fmtPct(t.progress)})`);
      ctx.log(`rate    ▼ ${fmtSpeed(t.downloadBps)}   ▲ ${fmtSpeed(t.uploadBps)}   eta ${fmtEta(t.etaSecs)}`);
      ctx.log(`peers   ${t.peersLive} live / ${t.peersQueued} queued / ${t.peersSeen} seen`);
      ctx.log(`files   ${t.fileCount}`);
      ctx.log(`dir     ${t.outputDir}`);
    },
  },
  {
    name: "reveal",
    args: "opt",
    aliases: ["show"],
    usage: "/reveal [id]",
    about: "reveal the download folder in the file manager",
    run: async (ctx, [target]) => {
      const id = resolveId(ctx, target);
      const dir = await api.outputDir(id);
      await tryOpen(revealInDir, dir, ctx, "revealed");
    },
  },
  {
    name: "sel",
    args: "req",
    aliases: ["select"],
    usage: "/sel <id>",
    about: "select a torrent for id-less commands",
    run: (ctx, [target]) => {
      const id = resolveId(ctx, target);
      ctx.state.selectedId = id;
      ctx.log(`selected [${id}] ${torrent(ctx, id)?.name ?? ""}`, "ok");
    },
  },
  {
    name: "config",
    args: "none",
    aliases: ["cfg"],
    usage: "/config",
    about: "print current settings",
    run: (ctx) => {
      const c = ctx.state.config;
      ctx.log(`dir     ${c.downloadDir}`, "ok");
      ctx.log(`down    ${rateLabel(c.maxDownloadBps)}`);
      ctx.log(`up      ${rateLabel(c.maxUploadBps)}`);
      ctx.log(`theme   ${c.theme}`);
    },
  },
  {
    name: "set",
    args: "req",
    aliases: [],
    usage: "/set <dir|down|up|theme|crt> …",
    about: "change a setting",
    run: async (ctx, [key, ...rest]) => {
      const value = rest.join(" ").trim();
      switch (key) {
        case "dir": {
          const dir = value || (await pickDirectory(ctx.state.config.downloadDir));
          if (!dir) return ctx.log("cancelled");
          const path = typeof dir === "string" ? dir : dir.path;
          await ctx.applyConfig({ ...ctx.state.config, downloadDir: path });
          return ctx.log(`download dir → ${path}`, "ok");
        }
        case "down": {
          const bps = parseRate(value);
          await ctx.applyConfig({ ...ctx.state.config, maxDownloadBps: bps });
          return ctx.log(`download limit → ${rateLabel(bps)}`, "ok");
        }
        case "up": {
          const bps = parseRate(value);
          await ctx.applyConfig({ ...ctx.state.config, maxUploadBps: bps });
          return ctx.log(`upload limit → ${rateLabel(bps)}`, "ok");
        }
        case "theme": {
          if (!THEMES.includes(value)) throw new Error(`theme must be one of: ${THEMES.join(", ")}`);
          await ctx.applyConfig({ ...ctx.state.config, theme: value });
          document.documentElement.dataset.theme = value;
          return ctx.log(`theme → ${value}`, "ok");
        }
        case "crt": {
          const on = value !== "off";
          document.documentElement.dataset.crt = on ? "on" : "off";
          return ctx.log(`crt overlay → ${on ? "on" : "off"}`, "ok");
        }
        default:
          throw new Error("usage: /set <dir|down|up|theme|crt> <value>");
      }
    },
  },
  {
    name: "openfolder",
    args: "none",
    aliases: ["cd"],
    usage: "/openfolder",
    about: "open the download folder",
    run: async (ctx) => {
      await tryOpen(openPath, ctx.state.config.downloadDir, ctx, "opened");
    },
  },
  {
    name: "clear",
    args: "none",
    aliases: ["cls"],
    usage: "/clear",
    about: "clear the console",
    run: () => clearLog(),
  },
];

const BY_NAME = new Map();
for (const c of COMMANDS) {
  BY_NAME.set(c.name, c);
  for (const a of c.aliases) BY_NAME.set(a, c);
}

export const lookup = (name) => BY_NAME.get(name);
