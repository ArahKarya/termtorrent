// DOM rendering. Rows are keyed by torrent id and reused across ticks so a
// 700ms refresh does not rebuild the whole list.

import { bar, barWidth, fmtBytes, fmtEta, fmtPath, fmtPct, fmtSpeed } from "./format.js";

const el = (id) => document.getElementById(id);

const dom = {
  list: el("torrents"),
  empty: el("empty"),
  screen: el("screen"),
  console: el("console"),
  badge: el("conn-badge"),
  count: el("s-count"),
  down: el("s-down"),
  up: el("s-up"),
  peers: el("s-peers"),
  port: el("s-port"),
  dht: el("s-dht"),
  dir: el("s-dir"),
};

const rows = new Map();
let width = barWidth(window.innerWidth);

window.addEventListener("resize", () => {
  width = barWidth(window.innerWidth);
});

function buildRow(id) {
  const li = document.createElement("li");
  li.className = "row";
  li.dataset.id = String(id);
  li.innerHTML = `
    <div class="row-head">
      <span class="idx"></span>
      <span class="name"></span>
      <span class="state"></span>
    </div>
    <div class="bar-line">
      <span class="bar"><span class="fill"></span><span class="rest"></span></span>
      <span class="pct"></span>
      <span class="sizes"></span>
    </div>
    <div class="meta"></div>`;

  return {
    li,
    idx: li.querySelector(".idx"),
    name: li.querySelector(".name"),
    state: li.querySelector(".state"),
    fill: li.querySelector(".fill"),
    rest: li.querySelector(".rest"),
    pct: li.querySelector(".pct"),
    sizes: li.querySelector(".sizes"),
    meta: li.querySelector(".meta"),
  };
}

function metaHtml(t) {
  if (t.error) return `<span class="err">! ${escapeHtml(t.error)}</span>`;
  const peers = `${t.peersLive}<span class="dim">/${t.peersSeen}</span>`;
  const eta = t.state === "downloading" ? `eta <span class="v">${fmtEta(t.etaSecs)}</span>` : "";
  return [
    `▼ <span class="v">${fmtSpeed(t.downloadBps)}</span>`,
    `▲ <span class="v">${fmtSpeed(t.uploadBps)}</span>`,
    `peers <span class="v">${peers}</span>`,
    eta,
    `up <span class="v">${fmtBytes(t.uploadedBytes)}</span>`,
    t.fileCount > 1 ? `<span class="v">${t.fileCount}</span> files` : "",
  ]
    .filter(Boolean)
    .map((s) => `<span>${s}</span>`)
    .join("");
}

function escapeHtml(s) {
  return String(s).replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
  );
}

/** Paint one frame. `selectedId` may be null. */
export function renderSnapshot(snapshot, selectedId) {
  const { torrents, session } = snapshot;

  dom.count.textContent = String(session.torrentCount);
  dom.down.textContent = fmtSpeed(session.downloadBps);
  dom.up.textContent = fmtSpeed(session.uploadBps);
  dom.peers.textContent = String(session.peersLive);
  dom.port.textContent = session.listenPort ?? "—";
  dom.dht.textContent = session.dhtEnabled ? "on" : "off";

  const hasTorrents = torrents.length > 0;
  dom.empty.classList.toggle("hidden", hasTorrents);
  dom.list.classList.toggle("hidden", !hasTorrents);

  const seen = new Set();
  torrents.forEach((t) => {
    seen.add(t.id);
    let row = rows.get(t.id);
    if (!row) {
      row = buildRow(t.id);
      rows.set(t.id, row);
      dom.list.appendChild(row.li);
    }

    const { filled, rest } = bar(t.progress, width);
    setText(row.idx, `[${t.id}]`);
    setText(row.name, t.name);
    setText(row.state, t.state);
    setText(row.fill, filled);
    setText(row.rest, rest);
    setText(row.pct, fmtPct(t.progress));
    setText(
      row.sizes,
      `${fmtBytes(t.progressBytes)} / ${t.totalBytes ? fmtBytes(t.totalBytes) : "?"}`,
    );

    if (row.state.dataset.s !== t.state) {
      row.state.dataset.s = t.state;
      row.li.dataset.s = t.state;
    }
    const meta = metaHtml(t);
    if (row.meta.dataset.sig !== meta) {
      row.meta.dataset.sig = meta;
      row.meta.innerHTML = meta;
    }
    row.li.classList.toggle("selected", t.id === selectedId);
    row.li.title = `${t.name}\n${t.infoHash}\n${t.outputDir}`;
  });

  for (const [id, row] of rows) {
    if (!seen.has(id)) {
      row.li.remove();
      rows.delete(id);
    }
  }
}

function setText(node, value) {
  if (node.textContent !== value) node.textContent = value;
}

export function setDownloadDir(path) {
  dom.dir.textContent = fmtPath(path);
  dom.dir.title = path;
}

export function setBadge(text, state = "ok") {
  dom.badge.textContent = text;
  dom.badge.dataset.state = state;
}

/** Append a line to the console pane. */
export function log(text, kind = "") {
  const line = document.createElement("div");
  line.className = `log ${kind}`.trim();
  line.textContent = text;
  dom.console.appendChild(line);
  while (dom.console.childElementCount > 300) dom.console.firstElementChild.remove();
  dom.console.scrollTop = dom.console.scrollHeight;
}

export function clearLog() {
  dom.console.replaceChildren();
}

/** Wire row clicks to a selection callback. */
export function onRowClick(handler) {
  dom.list.addEventListener("click", (e) => {
    const li = e.target.closest(".row");
    if (li) handler(Number(li.dataset.id));
  });
}
