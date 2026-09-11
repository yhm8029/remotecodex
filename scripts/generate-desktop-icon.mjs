import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SIZE = 32;
const W = SIZE, H = SIZE;
const BG = [0x14, 0x1C, 0x2E, 0xFF];
const FG = [0x7D, 0xF0, 0xB6, 0xFF];
const MUTED = [0x4A, 0x55, 0x68, 0xFF];

function setPx(bmp, x, y, c) {
  const i = (y * W + x) * 4;
  bmp[i] = c[2]; bmp[i + 1] = c[1]; bmp[i + 2] = c[0]; bmp[i + 3] = c[3];
}

const pixels = new Uint8Array(W * H * 4);
for (let i = 0; i < pixels.length; i += 4) {
  pixels[i] = BG[2]; pixels[i + 1] = BG[1]; pixels[i + 2] = BG[0]; pixels[i + 3] = BG[3];
}

const drawGlyph = (gx, gy, color, draw) => {
  for (let row = 0; row < 7; row++) {
    for (let col = 0; col < 5; col++) {
      if (draw[row] & (1 << (4 - col))) setPx(pixels, gx + col, gy + row, color);
    }
  }
};

const prompt = [0b10000, 0b01000, 0b00100, 0b00010, 0b00100, 0b01000, 0b10000];
drawGlyph(8, 12, FG, prompt);

const underscore = [0, 0, 0, 0, 0, 0, 0b11111];
drawGlyph(16, 12, MUTED, underscore);

const dibSize = 40;
const imgBytes = W * H * 4;
const maskStride = Math.ceil(W / 32) * 4;
const maskBytes = maskStride * H;
const imgOffset = 6 + 16;

const dir = Buffer.alloc(6);
dir.writeUInt16LE(0, 0);
dir.writeUInt16LE(1, 2);
dir.writeUInt16LE(1, 4);

const entry = Buffer.alloc(16);
entry.writeUInt8(W === 256 ? 0 : W, 0);
entry.writeUInt8(H === 256 ? 0 : H, 1);
entry.writeUInt8(0, 2);
entry.writeUInt8(0, 3);
entry.writeUInt16LE(1, 4);
entry.writeUInt16LE(32, 6);
entry.writeUInt32LE(dibSize + imgBytes + maskBytes, 8);
entry.writeUInt32LE(imgOffset, 12);

const bih = Buffer.alloc(dibSize);
bih.writeUInt32LE(dibSize, 0);
bih.writeInt32LE(W, 4);
bih.writeInt32LE(H * 2, 8);
bih.writeUInt16LE(1, 12);
bih.writeUInt16LE(32, 14);
bih.writeUInt32LE(0, 16);
bih.writeUInt32LE(imgBytes, 20);
bih.writeInt32LE(0, 24);
bih.writeInt32LE(0, 28);
bih.writeUInt32LE(0, 32);
bih.writeUInt32LE(0, 36);

const img = Buffer.alloc(imgBytes);
for (let y = 0; y < H; y++) {
  const src = (H - 1 - y) * W * 4;
  img.set(pixels.subarray(src, src + W * 4), y * W * 4);
}

const mask = Buffer.alloc(maskBytes);

const ico = Buffer.concat([dir, entry, bih, img, mask]);

const here = dirname(fileURLToPath(import.meta.url));
const outPath = resolve(here, '..', 'apps', 'desktop', 'src-tauri', 'icons', 'icon.ico');
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, ico);
console.log('wrote', outPath, ico.length, 'bytes');
