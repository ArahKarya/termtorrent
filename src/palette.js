// Slash-command palette: the popup that appears above the prompt when the
// line starts with "/", in the spirit of Claude Code's command menu.

import { COMMANDS } from "./commands.js";

const MAX_VISIBLE = 8;

const root = document.getElementById("palette");

let matches = [];
let cursor = 0;
let onAccept = () => {};

/** Commands whose prefix matches, de-duplicated across aliases. */
function search(token) {
  const q = token.toLowerCase();
  // Bare "/" lists everything in the registry's curated order.
  if (!q) return COMMANDS.map((cmd) => ({ cmd, shown: cmd.name, exact: false }));

  const scored = [];
  for (const cmd of COMMANDS) {
    const names = [cmd.name, ...cmd.aliases];
    const hit = names.find((n) => n.startsWith(q));
    if (hit) scored.push({ cmd, shown: hit, exact: hit === q });
  }
  // Exact matches first, then shortest name, then alphabetical.
  return scored.sort(
    (a, b) =>
      Number(b.exact) - Number(a.exact) ||
      a.shown.length - b.shown.length ||
      a.shown.localeCompare(b.shown),
  );
}

function paint() {
  root.replaceChildren();
  matches.forEach((m, i) => {
    const row = document.createElement("div");
    row.className = `pal-row${i === cursor ? " sel" : ""}`;
    row.dataset.index = String(i);

    const name = document.createElement("span");
    name.className = "pal-name";
    name.textContent = `/${m.shown}`;

    const about = document.createElement("span");
    about.className = "pal-about";
    about.textContent = m.cmd.about;

    const hint = document.createElement("span");
    hint.className = "pal-hint";
    hint.textContent = m.cmd.args === "none" ? "" : m.cmd.usage.replace(/^\/\S+\s*/, "");

    row.append(name, about, hint);
    root.appendChild(row);
  });

  root.classList.toggle("hidden", matches.length === 0);
  const sel = root.children[cursor];
  if (sel) sel.scrollIntoView({ block: "nearest" });
  root.style.maxHeight = `${MAX_VISIBLE * 24}px`;
}

/** Re-evaluate the palette for the current input value. */
export function update(value) {
  const m = /^\/(\S*)$/.exec(value);
  matches = m ? search(m[1]) : [];
  cursor = 0;
  paint();
}

export function isOpen() {
  return matches.length > 0;
}

export function close() {
  matches = [];
  paint();
}

export function move(delta) {
  if (!matches.length) return;
  cursor = (cursor + delta + matches.length) % matches.length;
  paint();
}

/** The command currently highlighted, or null. */
export function selected() {
  return matches[cursor] ?? null;
}

/** Register the callback fired when a row is clicked. */
export function onPick(handler) {
  onAccept = handler;
  root.addEventListener("mousedown", (e) => {
    const row = e.target.closest(".pal-row");
    if (!row) return;
    e.preventDefault(); // keep focus in the input
    cursor = Number(row.dataset.index);
    onAccept(matches[cursor]);
  });
  root.addEventListener("mousemove", (e) => {
    const row = e.target.closest(".pal-row");
    if (!row || Number(row.dataset.index) === cursor) return;
    cursor = Number(row.dataset.index);
    paint();
  });
}
