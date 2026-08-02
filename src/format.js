// Pure formatting helpers — no DOM, no Tauri.

const UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

/** 1536 -> "1.5 KiB" */
export function fmtBytes(n) {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  let i = 0;
  let v = n;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i += 1;
  }
  const digits = v >= 100 || i === 0 ? 0 : 1;
  return `${v.toFixed(digits)} ${UNITS[i]}`;
}

/** bytes/sec -> "4.2 MiB/s" */
export function fmtSpeed(bps) {
  return `${fmtBytes(bps)}/s`;
}

/** seconds -> "1h04m", "5m12s", "42s" */
export function fmtEta(secs) {
  if (secs == null || !Number.isFinite(secs)) return "—";
  if (secs >= 86400) return `${Math.floor(secs / 86400)}d${pad(Math.floor((secs % 86400) / 3600))}h`;
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h${pad(Math.floor((secs % 3600) / 60))}m`;
  if (secs >= 60) return `${Math.floor(secs / 60)}m${pad(secs % 60)}s`;
  return `${Math.floor(secs)}s`;
}

function pad(n) {
  return String(Math.floor(n)).padStart(2, "0");
}

/** 0.6142 -> " 61.4%" (fixed width so columns stay aligned) */
export function fmtPct(progress) {
  return `${(progress * 100).toFixed(1).padStart(5, " ")}%`;
}

const EIGHTHS = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/**
 * Build an eighth-resolution block progress bar.
 * Returns the filled and remaining halves separately so they can be coloured
 * independently without an extra DOM node per cell.
 */
export function bar(progress, width) {
  const clamped = Math.max(0, Math.min(1, progress || 0));
  const exact = clamped * width;
  const full = Math.floor(exact);
  const partial = EIGHTHS[Math.round((exact - full) * 8) % 8];
  const filled = "█".repeat(Math.min(full, width)) + (full < width ? partial : "");
  const rest = "░".repeat(Math.max(0, width - filled.length));
  return { filled, rest };
}

/** Fit a bar width to the available pixel width, within sane bounds. */
export function barWidth(pixels) {
  return Math.max(14, Math.min(48, Math.floor(pixels / 26)));
}

/** Shorten a long path for the status bar. */
export function fmtPath(path, max = 46) {
  if (!path || path.length <= max) return path || "—";
  return `…${path.slice(path.length - max + 1)}`;
}
