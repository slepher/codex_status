import fs from 'node:fs';
import zlib from 'node:zlib';
import { pathToFileURL } from 'node:url';

const WIDTH = 200;
const HEIGHT = 200;
const WHITE = 255;
const BLACK = 0;
const ROOT = new URL('../', import.meta.url);
const TEMPLATE_URL = new URL('tools/test-bridge/templates/quad.json', ROOT);
const OUTPUT_DIR = new URL('artifacts/', ROOT);
const FONT_NAMES = ['f8', 'f12', 'f16', 'f20', 'f24'];

function loadTemplate() {
  return JSON.parse(fs.readFileSync(TEMPLATE_URL, 'utf8'));
}

// Parse the checked-in C font tables and their sFONT metrics so the preview
// exercises the same glyph bytes and fixed advances as the firmware.
function loadFont(name) {
  if (!FONT_NAMES.includes(name)) throw new Error(`unknown firmware font: ${name}`);
  const number = name.slice(1);
  const source = fs.readFileSync(new URL(`src/font${number}.cpp`, ROOT), 'utf8');
  const tableMatch = source.match(new RegExp(
    `const\\s+uint8_t\\s+Font${number}_Table\\s*\\[\\]\\s*=\\s*\\{([\\s\\S]*?)\\};`,
  ));
  const structMatch = source.match(new RegExp(`sFONT\\s+Font${number}\\s*=\\s*\\{([\\s\\S]*?)\\};`));
  if (!tableMatch || !structMatch) throw new Error(`cannot parse firmware font ${name}`);
  const tableText = tableMatch[1].replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  const bytes = [...tableText.matchAll(/0x[0-9a-f]+/gi)].map(([value]) => Number.parseInt(value, 16));
  const metrics = Object.fromEntries([...structMatch[1].matchAll(/(\d+)\s*,\s*\/\*\s*(Width|Height)/g)]
    .map(([, value, key]) => [key.toLowerCase(), Number(value)]));
  if (!Number.isInteger(metrics.width) || !Number.isInteger(metrics.height)) {
    throw new Error(`missing metrics for firmware font ${name}`);
  }
  const stride = Math.ceil(metrics.width / 8);
  const expected = 95 * metrics.height * stride;
  if (bytes.length !== expected) {
    throw new Error(`font ${name} table has ${bytes.length} bytes; expected ${expected}`);
  }
  return Object.freeze({ name, width: metrics.width, height: metrics.height, stride, bytes });
}

const FONTS = Object.freeze(Object.fromEntries(FONT_NAMES.map((name) => [name, loadFont(name)])));

function parseBind(path) {
  const direct = new Set([
    'account.plan', 'bridge.label', 'bridge.hostId', 'server_time',
    'resetCredits.availableCount', 'resetCredits.nextExpiresAt',
    'device.channel', 'device.ip', 'device.sync_hhmm', 'device.battery',
    'device.state', 'device.offline_mins',
  ]);
  if (direct.has(path)) return { kind: path };
  const match = /^buckets\[([^\]]+)\]\.((?:weekly|monthly|5h|primary|secondary|windows\[\d+\]))\.(usedPercent|remaining|resetsAt|windowMins)$/.exec(path);
  if (!match) return null;
  const [, bucket, winToken, field] = match;
  const win = /^windows\[(\d+)\]$/.test(winToken)
    ? { mode: 'index', index: Number(winToken.slice(8, -1)) }
    : { mode: winToken };
  return { kind: 'bucket', bucket, win, field };
}

function isEpochBind(spec) {
  return spec && (spec.kind === 'server_time' || spec.kind === 'resetCredits.nextExpiresAt'
    || (spec.kind === 'bucket' && spec.field === 'resetsAt'));
}

function rectOk(rect) {
  if (!Array.isArray(rect) || rect.length < 4 || !rect.every(Number.isInteger)) return false;
  const [x, y, w, h] = rect;
  return w > 0 && h > 0 && x < WIDTH && y < HEIGHT && x + w > 0 && y + h > 0;
}

function regionOk(region) {
  return Array.isArray(region) && region.length === 4 && region.every(Number.isInteger)
    && region[0] >= 0 && region[1] >= 0 && region[2] > 0 && region[3] > 0
    && region[0] + region[2] <= WIDTH && region[1] + region[3] <= HEIGHT;
}

function validateCondition(element) {
  if (!Object.hasOwn(element, 'when')) return;
  const condition = element.when;
  if (!condition || typeof condition !== 'object' || Array.isArray(condition)
    || Object.keys(condition).length !== 2 || !Object.hasOwn(condition, 'bind')
    || !Object.hasOwn(condition, 'exists') || typeof condition.bind !== 'string'
    || typeof condition.exists !== 'boolean' || !parseBind(condition.bind)) {
    throw new Error('invalid when condition');
  }
}

function validateTemplate(template) {
  if (template.schema !== 1 || template.canvas?.w !== WIDTH || template.canvas?.h !== HEIGHT
    || !Array.isArray(template.elements) || template.elements.length === 0) {
    throw new Error('invalid template canvas or elements');
  }
  for (const element of template.elements) {
    validateCondition(element);
    if (element.type === 'text') {
      if (!FONTS[element.font] || (typeof element.bind !== 'string' && typeof element.text !== 'string')
        || (!element.bind && !element.text)) throw new Error('invalid text element');
      const spec = element.bind ? parseBind(element.bind) : null;
      if (element.bind && !spec) throw new Error(`invalid bind ${element.bind}`);
      if (Object.hasOwn(element, 'scale')
        && (!Number.isInteger(element.scale) || element.scale < 1 || element.scale > 3)) {
        throw new Error('invalid text scale');
      }
      if (Object.hasOwn(element, 'region') && !regionOk(element.region)) throw new Error('invalid text region');
      if (Object.hasOwn(element, 'align')
        && (!Object.hasOwn(element, 'region') || !['left', 'center', 'right'].includes(element.align))) {
        throw new Error('invalid text alignment');
      }
      if (Object.hasOwn(element, 'time_format')
        && (!isEpochBind(spec) || !['date', 'hhmm'].includes(element.time_format))) {
        throw new Error('invalid time format');
      }
    } else if (element.type === 'rect') {
      if (!rectOk(element.rect)) throw new Error('invalid rectangle');
    } else if (element.type === 'icon') {
      if (!Number.isInteger(element.x) || !Number.isInteger(element.y)
        || !Number.isInteger(element.w) || !Number.isInteger(element.h)
        || element.w <= 0 || element.h <= 0 || !rectOk([element.x, element.y, element.w, element.h])) {
        throw new Error('invalid icon geometry');
      }
      const bits = Buffer.from(element.bits || '', 'base64');
      if (!element.bits || bits.length !== Math.ceil(element.w / 8) * element.h) {
        throw new Error('invalid icon bits');
      }
    } else if (element.type === 'bar') {
      if (!parseBind(element.bind) || !rectOk(element.rect)) throw new Error('invalid bar');
    } else if (element.type === 'line') {
      if (![element.x1, element.y1, element.x2, element.y2].every(Number.isInteger)) throw new Error('invalid line');
    } else {
      throw new Error(`unknown element ${element.type}`);
    }
  }
  return template;
}

function asciiOnly(value) {
  return [...String(value)].map((character) => (
    character.charCodeAt(0) >= 32 && character.charCodeAt(0) <= 126 ? character : '?'
  )).join('');
}

function measureText(font, value, scale) {
  return asciiOnly(value).length * font.width * scale;
}

function createRaster() {
  return { width: WIDTH, height: HEIGHT, pixels: Buffer.alloc(WIDTH * HEIGHT, WHITE) };
}

function paintPixel(raster, x, y, value) {
  if (!Number.isInteger(x) || !Number.isInteger(y) || x < 0 || y < 0 || x >= WIDTH || y >= HEIGHT) {
    throw new Error(`pixel out of bounds: ${x},${y}`);
  }
  raster.pixels[y * WIDTH + x] = value;
}

function fillRect(raster, x, y, width, height, value) {
  for (let row = y; row < y + height; row += 1) {
    for (let column = x; column < x + width; column += 1) paintInk(raster, column, row, value);
  }
}

// Firmware paints lines/rectangles through Waveshare Paint_DrawPoint with the
// default DOT_FILL_AROUND style: every point lands at (x-1, y-1) and points
// outside the canvas are dropped. Text and icons bypass that path and use
// Paint_SetPixel directly.
function paintInk(raster, x, y, value) {
  const px = x - 1;
  const py = y - 1;
  if (px < 0 || py < 0 || px >= WIDTH || py >= HEIGHT) return;
  paintPixel(raster, px, py, value);
}

function colorValue(value, fallback = BLACK) {
  if (value === 'white' || value === 1) return WHITE;
  if (value === 'black' || value === 'yellow' || value === 'red' || value === 0) return BLACK;
  if (value === 'none') return null;
  return fallback === WHITE || fallback === 1 ? WHITE : BLACK;
}

function formatEpoch(epoch, format = 'date') {
  if (!Number.isInteger(epoch) || epoch <= 0) return '--';
  const date = new Date((epoch + 8 * 60 * 60) * 1000);
  if (Number.isNaN(date.getTime())) return '--';
  const pad = (value) => String(value).padStart(2, '0');
  const hhmm = `${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}`;
  return format === 'hhmm' ? hhmm : `${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())} ${hhmm}`;
}

function findWindow(usage, spec) {
  if (!spec || spec.kind !== 'bucket') return null;
  const bucket = usage?.buckets?.find((candidate) => candidate?.id === spec.bucket);
  if (!bucket || !Array.isArray(bucket.windows)) return null;
  if (spec.win.mode === 'index') return bucket.windows[spec.win.index] || null;
  const expected = spec.win.mode === 'weekly' ? (window) => window.windowMins >= 10080
    : spec.win.mode === 'monthly' ? (window) => window.windowMins >= 43200
    : spec.win.mode === '5h' ? (window) => window.windowMins === 300 : null;
  if (expected) return bucket.windows.find(expected) || null;
  return bucket.windows[spec.win.mode === 'primary' ? 0 : 1] || null;
}

function bindResult(spec, usage, env, format = 'date') {
  if (!spec) return { exists: false, value: '--' };
  if (spec.kind === 'account.plan' || spec.kind === 'bridge.label' || spec.kind === 'bridge.hostId') {
    const [parent, key] = spec.kind.split('.');
    const value = usage?.[parent]?.[key];
    return { exists: value !== undefined && value !== null, value: value ?? '--' };
  }
  if (spec.kind === 'server_time') {
    const value = usage?.server_time;
    return { exists: value !== undefined && value !== null, value: formatEpoch(value, format) };
  }
  if (spec.kind === 'resetCredits.availableCount' || spec.kind === 'resetCredits.nextExpiresAt') {
    const key = spec.kind.endsWith('availableCount') ? 'availableCount' : 'nextExpiresAt';
    const value = usage?.resetCredits?.[key];
    return { exists: value !== undefined && value !== null, value: key === 'nextExpiresAt' ? formatEpoch(value, format) : (value ?? '--') };
  }
  if (spec.kind === 'device.channel' || spec.kind === 'device.ip' || spec.kind === 'device.sync_hhmm') {
    const key = spec.kind.slice('device.'.length);
    const value = env[key];
    return { exists: typeof value === 'string' && value.length > 0, value: value || '--' };
  }
  if (spec.kind === 'device.battery') {
    return { exists: Number.isInteger(env.battery) && env.battery >= 0, value: env.battery >= 0 ? env.battery : '--' };
  }
  if (spec.kind === 'device.state') {
    const value = env.state;
    return { exists: typeof value === 'string' && value.length > 0, value: value || '--' };
  }
  if (spec.kind === 'device.offline_mins') {
    // Device-side policy decides when the bridge counts as unreachable
    // (heartbeat grace); the preview treats any non-negative value as present.
    const exists = Number.isInteger(env.offline_mins) && env.offline_mins >= 0;
    return { exists, value: exists ? env.offline_mins : '--' };
  }
  const window = findWindow(usage, spec);
  if (!window) return { exists: false, value: '--' };
  const fieldExists = spec.field === 'remaining' ? window.usedPercent !== undefined && window.usedPercent !== null
    : window[spec.field] !== undefined && window[spec.field] !== null;
  if (!fieldExists) return { exists: false, value: '--' };
  if (spec.field === 'remaining') return { exists: true, value: 100 - window.usedPercent };
  if (spec.field === 'resetsAt') return { exists: true, value: formatEpoch(window.resetsAt, format) };
  return { exists: true, value: window[spec.field] };
}

function conditionMatches(element, usage, env) {
  if (!Object.hasOwn(element, 'when')) return true;
  const condition = element.when;
  const result = bindResult(parseBind(condition.bind), usage, env);
  return result.exists === condition.exists;
}

function drawGlyphText(raster, value, font, x, y, scale, foreground, background, region) {
  const text = asciiOnly(value);
  const clip = region ? { x: region[0], y: region[1], w: region[2], h: region[3] } : null;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    const offset = (code - 32) * font.height * font.stride;
    for (let row = 0; row < font.height; row += 1) {
      for (let col = 0; col < font.width; col += 1) {
        const ink = (font.bytes[offset + row * font.stride + Math.floor(col / 8)]
          & (0x80 >> (col % 8))) !== 0;
        if (!ink && background === null) continue;
        for (let sy = 0; sy < scale; sy += 1) {
          for (let sx = 0; sx < scale; sx += 1) {
            const px = x + index * font.width * scale + col * scale + sx;
            const py = y + row * scale + sy;
            if (px < 0 || py < 0 || px >= WIDTH || py >= HEIGHT) continue;
            if (clip && (px < clip.x || py < clip.y || px >= clip.x + clip.w || py >= clip.y + clip.h)) continue;
            paintPixel(raster, px, py, ink ? foreground : background);
          }
        }
      }
    }
  }
}

function drawText(raster, element, usage, env) {
  const font = FONTS[element.font];
  const spec = element.bind ? parseBind(element.bind) : null;
  const result = element.bind ? bindResult(spec, usage, env, element.time_format || 'date') : { value: element.text };
  const value = asciiOnly(`${element.prefix || ''}${result.value}${element.suffix || ''}`);
  const requestedScale = element.scale || 1;
  const region = element.region || null;
  let scale = requestedScale;
  let width = measureText(font, value, scale);
  while (region && scale > 1 && (width > region[2] || font.height * scale > region[3])) {
    scale -= 1;
    width = measureText(font, value, scale);
  }
  let x = element.x || 0;
  let y = element.y || 0;
  if (region) {
    x = region[0];
    if (element.align === 'center') x += Math.floor((region[2] - width) / 2);
    else if (element.align === 'right') x += region[2] - width;
    y = region[1] + Math.floor((region[3] - font.height * scale) / 2);
  }
  drawGlyphText(raster, value, font, x, y, scale,
    colorValue(element.color, BLACK), colorValue(element.bg, WHITE), region);
}

function drawIcon(raster, element) {
  const bits = Buffer.from(element.bits, 'base64');
  const stride = Math.ceil(element.w / 8);
  const foreground = colorValue(element.color, BLACK);
  for (let row = 0; row < element.h; row += 1) {
    for (let col = 0; col < element.w; col += 1) {
      if (bits[row * stride + Math.floor(col / 8)] & (0x80 >> (col % 8))) {
        paintPixel(raster, element.x + col, element.y + row, foreground);
      }
    }
  }
}

function render(template, usage = {}, env = {}) {
  validateTemplate(template);
  const raster = createRaster();
  for (const element of template.elements) {
    if (!conditionMatches(element, usage, env)) continue;
    if (element.type === 'rect') {
      const [x, y, w, h] = element.rect;
      if (element.fill) fillRect(raster, x, y, w, h - 1, colorValue(element.color, BLACK));
      else {
        fillRect(raster, x, y, w, 1, colorValue(element.color, BLACK));
        fillRect(raster, x, y + h - 1, w, 1, colorValue(element.color, BLACK));
        fillRect(raster, x, y, 1, h, colorValue(element.color, BLACK));
        fillRect(raster, x + w - 1, y, 1, h, colorValue(element.color, BLACK));
      }
    } else if (element.type === 'text') {
      drawText(raster, element, usage, env);
    } else if (element.type === 'icon') {
      drawIcon(raster, element);
    } else if (element.type === 'bar') {
      const [x, y, w, h] = element.rect;
      const result = bindResult(parseBind(element.bind), usage, env);
      const value = Number(result.value) || 0;
      if (element.bg !== 'none') fillRect(raster, x, y, w, h - 1, colorValue(element.bg, WHITE));
      const filled = Math.max(0, Math.min(w, Math.round(w * value / (element.max || 100))));
      if (filled) fillRect(raster, x, y, filled, h - 1, colorValue(element.fg, BLACK));
    } else if (element.type === 'line') {
      const dx = Math.abs(element.x2 - element.x1);
      const dy = -Math.abs(element.y2 - element.y1);
      const sx = element.x1 < element.x2 ? 1 : -1;
      const sy = element.y1 < element.y2 ? 1 : -1;
      let error = dx + dy;
      let x = element.x1;
      let y = element.y1;
      while (true) {
        paintInk(raster, x, y, colorValue(element.color, BLACK));
        if (x === element.x2 && y === element.y2) break;
        const twice = 2 * error;
        if (twice >= dy) { error += dy; x += sx; }
        if (twice <= dx) { error += dx; y += sy; }
      }
    }
  }
  for (const value of raster.pixels) if (value !== WHITE && value !== BLACK) throw new Error('palette');
  return raster;
}

function crc32(data) {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc & 1) ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, 'ascii');
  const chunk = Buffer.alloc(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return chunk;
}

function encodePng(raster) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(WIDTH, 0);
  ihdr.writeUInt32BE(HEIGHT, 4);
  ihdr[8] = 8;
  const scanlines = Buffer.alloc(HEIGHT * (WIDTH + 1));
  for (let row = 0; row < HEIGHT; row += 1) {
    scanlines[row * (WIDTH + 1)] = 0;
    raster.pixels.copy(scanlines, row * (WIDTH + 1) + 1, row * WIDTH, (row + 1) * WIDTH);
  }
  const compressed = zlib.deflateSync(scanlines, { level: 9, strategy: zlib.constants.Z_FIXED });
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr), pngChunk('IDAT', compressed), pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function usageFixture({ weeklyUsed = 91, weeklyMins = 10080, fiveHourUsed = null, plan = 'prolite', label = 'ALICE', resetCount = 0 } = {}) {
  const usage = {
    schema: 1,
    server_time: 1789543987,
    bridge: { label, hostId: '1a2b' },
    account: { plan },
    buckets: [{ id: 'codex', windows: [{ kind: 'weekly', resetsAt: 1789805806, usedPercent: weeklyUsed, windowMins: weeklyMins }] }],
  };
  if (resetCount > 0) usage.resetCredits = { availableCount: resetCount, nextExpiresAt: 1789561987 };
  if (fiveHourUsed !== null) usage.buckets[0].windows.unshift({
    kind: '5h', resetsAt: 1789561987, usedPercent: fiveHourUsed, windowMins: 300,
  });
  // This Spark bucket proves that a missing Codex 5h window is not borrowed.
  usage.buckets.push({ id: 'codex_bengalfox', windows: [{ kind: '5h', resetsAt: 1789561987, usedPercent: 0, windowMins: 300 }] });
  return usage;
}

function writePreview(template, usage, env, name) {
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
  fs.writeFileSync(new URL(name, OUTPUT_DIR), encodePng(render(template, usage, env)));
  console.log(`Generated artifacts/${name} (200x200 firmware-font PNG)`);
}

function generate() {
  const template = validateTemplate(loadTemplate());
  const env = { channel: 'WIFI', ip: '192.168.1.50', sync_hhmm: '23:59', battery: 78, state: 'BLE OFF' };
  writePreview(template, usageFixture({ fiveHourUsed: 11 }), env, 'quad-preview.png');
  writePreview(template, usageFixture({ weeklyUsed: 0, fiveHourUsed: 0 }), env, 'quad-preview-100.png');
  writePreview(template, usageFixture({ weeklyUsed: 91, fiveHourUsed: null }), env, 'quad-preview-missing-5h.png');
  writePreview(template, usageFixture({ weeklyUsed: 2, fiveHourUsed: 11, plan: 'pro', label: 'VERY-LONG-BRIDGE-LABEL', resetCount: 1 }), env, 'quad-preview-longnames.png');
  const offlineEnv = { ...env, state: 'WIFI OFF', offline_mins: 138 };
  writePreview(template, usageFixture({ fiveHourUsed: 11 }), offlineEnv, 'quad-preview-offline.png');
  writePreview(template, usageFixture({ weeklyUsed: 91, fiveHourUsed: null }), { ...offlineEnv, battery: 21 }, 'quad-preview-offline-missing-5h.png');
  writePreview(template, usageFixture({ weeklyUsed: 12, weeklyMins: 43200, fiveHourUsed: null, plan: 'free' }), env, 'quad-preview-monthly.png');
}

export {
  BLACK, WHITE, FONTS, HEIGHT, TEMPLATE_URL, WIDTH, bindResult, formatEpoch, loadTemplate,
  measureText, parseBind, render, usageFixture, validateTemplate,
};

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) generate();
