// 纯 Node 生成 1024x1024 应用图标 PNG（不引入任何第三方依赖）
// 设计：深蓝→青色渐变圆角方块 + 三根递增的绿色信号条 + 白色基线
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const W = 1024;
const H = 1024;
const RADIUS = 190;

const raw = Buffer.alloc(H * (1 + W * 4)); // 每行 1 字节滤波器 + RGBA

function setPx(x, y, r, g, b, a) {
  const off = y * (1 + W * 4) + 1 + x * 4;
  raw[off] = r;
  raw[off + 1] = g;
  raw[off + 2] = b;
  raw[off + 3] = a;
}

/** 圆角矩形内部判定 */
function inside(x, y) {
  const cx = Math.min(Math.max(x, RADIUS), W - 1 - RADIUS);
  const cy = Math.min(Math.max(y, RADIUS), H - 1 - RADIUS);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= RADIUS * RADIUS;
}

function fillRect(x0, y0, x1, y1, r, g, b, a = 255) {
  const ys = Math.max(0, Math.min(H, y0));
  const ye = Math.max(0, Math.min(H, y1));
  const xs = Math.max(0, Math.min(W, x0));
  const xe = Math.max(0, Math.min(W, x1));
  for (let y = ys; y < ye; y++) {
    for (let x = xs; x < xe; x++) setPx(x, y, r, g, b, a);
  }
}

const lerp = (a, b, t) => Math.round(a + (b - a) * t);

// 1) 背景渐变（深蓝 → 青）
for (let y = 0; y < H; y++) {
  for (let x = 0; x < W; x++) {
    if (!inside(x, y)) {
      setPx(x, y, 0, 0, 0, 0);
      continue;
    }
    const t = (x + y) / (2 * (W - 1));
    setPx(x, y, lerp(12, 6, t), lerp(24, 145, t), lerp(52, 178, t), 255);
  }
}

// 2) 三根递增的信号条
fillRect(230, 640, 350, 830, 103, 232, 249); // 青
fillRect(452, 470, 572, 830, 52, 211, 153); // 翠绿
fillRect(674, 300, 794, 830, 74, 222, 128); // 亮绿

// 3) 白色基线
fillRect(200, 830, 824, 846, 255, 255, 255, 240);

// 4) 顶部白色脉冲点（点缀）
for (let y = 170; y < 290; y++) {
  for (let x = 462; x < 582; x++) {
    const dx = x - 522;
    const dy = y - 230;
    const d2 = dx * dx + dy * dy;
    if (d2 <= 48 * 48) setPx(x, y, 255, 255, 255, 255);
    else if (d2 <= 60 * 60 && inside(x, y)) setPx(x, y, 255, 255, 255, 90);
  }
}

// ---- PNG 编码 ----
const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const outPath = process.argv[2] || "assets/app-icon.png";
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, png);
console.log(`已生成图标：${outPath}（${png.length} 字节）`);
