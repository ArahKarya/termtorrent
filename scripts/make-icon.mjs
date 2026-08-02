// Generates the source app icon (1024x1024 PNG) with no external deps.
// Run: node scripts/make-icon.mjs  ->  app-icon.png
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const SIZE = 1024;
const BG = [11, 15, 14, 255];
const FRAME = [22, 32, 29, 255];
const GREEN = [79, 255, 176, 255];
const DIM = [32, 74, 62, 255];

const px = Buffer.alloc(SIZE * SIZE * 4);

const inside = (x, y) => x >= 0 && y >= 0 && x < SIZE && y < SIZE;

function set(x, y, [r, g, b, a]) {
  if (!inside(x, y)) return;
  const i = (y * SIZE + x) * 4;
  px[i] = r;
  px[i + 1] = g;
  px[i + 2] = b;
  px[i + 3] = a;
}

function rect(x0, y0, w, h, color, radius = 0) {
  for (let y = y0; y < y0 + h; y++) {
    for (let x = x0; x < x0 + w; x++) {
      if (radius > 0) {
        const dx = Math.max(x0 + radius - x, x - (x0 + w - 1 - radius), 0);
        const dy = Math.max(y0 + radius - y, y - (y0 + h - 1 - radius), 0);
        if (dx * dx + dy * dy > radius * radius) continue;
      }
      set(x, y, color);
    }
  }
}

// Thick line from (x1,y1) to (x2,y2).
function line(x1, y1, x2, y2, thickness, color) {
  const steps = Math.max(Math.abs(x2 - x1), Math.abs(y2 - y1)) * 2;
  const half = thickness / 2;
  for (let s = 0; s <= steps; s++) {
    const t = s / steps;
    const cx = x1 + (x2 - x1) * t;
    const cy = y1 + (y2 - y1) * t;
    for (let dy = -half; dy <= half; dy++) {
      for (let dx = -half; dx <= half; dx++) {
        if (dx * dx + dy * dy <= half * half) {
          set(Math.round(cx + dx), Math.round(cy + dy), color);
        }
      }
    }
  }
}

// Background + terminal window frame.
rect(0, 0, SIZE, SIZE, BG);
rect(96, 128, SIZE - 192, SIZE - 256, FRAME, 72);
rect(120, 152, SIZE - 240, 8, DIM);

// Prompt chevron ">".
line(300, 380, 440, 500, 52, GREEN);
line(440, 500, 300, 620, 52, GREEN);

// Cursor underscore next to the prompt.
rect(500, 578, 220, 44, GREEN, 14);

// Progress bar: filled portion bright, remainder dim.
rect(300, 720, 424, 56, GREEN, 16);
rect(736, 720, 188, 56, DIM, 16);

// --- PNG encoding -----------------------------------------------------------
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([len, body, crc]);
}

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return c ^ -1;
}

const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  px.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

writeFileSync(new URL("../app-icon.png", import.meta.url), png);
console.log(`wrote app-icon.png (${SIZE}x${SIZE}, ${png.length} bytes)`);
