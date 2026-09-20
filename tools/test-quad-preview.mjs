import assert from 'node:assert/strict';
import fs from 'node:fs';

import {
  BLACK, FONTS, HEIGHT, WIDTH, bindResult, formatEpoch, loadTemplate, measureText, parseBind, render,
  usageFixture, validateTemplate,
} from './generate-quad-preview.mjs';

const template = validateTemplate(loadTemplate());
const env = { channel: 'WIFI', ip: '192.168.1.50', sync_hhmm: '23:59', battery: 78, state: 'BLE OFF' };
const normal = render(template, usageFixture({ fiveHourUsed: 11 }), env);
const full = render(template, usageFixture({ weeklyUsed: 0, fiveHourUsed: 0 }), env);
const used100 = render(template, usageFixture({ weeklyUsed: 0, fiveHourUsed: 100 }), env);
const missingFiveHour = render(template, usageFixture({ weeklyUsed: 91, fiveHourUsed: null }), env);
const monthlyUsage = usageFixture({ weeklyUsed: 12, weeklyMins: 43200, fiveHourUsed: null, plan: 'free' });
const monthly = render(template, monthlyUsage, env);
const longNames = render(template, usageFixture({ weeklyUsed: 2, fiveHourUsed: 11, plan: 'pro', label: 'VERY-LONG-BRIDGE-LABEL' }), env);
const offlineEnv = { ...env, state: 'WIFI OFF', offline_mins: 138 };
const offline = render(template, usageFixture({ fiveHourUsed: 11 }), offlineEnv);
const offlineMissing = render(template, usageFixture({ weeklyUsed: 91, fiveHourUsed: null }), offlineEnv);

assert.equal(normal.width, WIDTH);
assert.equal(normal.height, HEIGHT);
for (const raster of [normal, full, missingFiveHour, longNames, offline, offlineMissing, monthly]) {
  assert.equal(raster.pixels.length, WIDTH * HEIGHT);
  for (const pixel of raster.pixels) assert.ok(pixel === 0 || pixel === 255, `palette ${pixel}`);
}

assert.equal(FONTS.f20.width, 14);
assert.equal(FONTS.f20.height, 20);
assert.equal(measureText(FONTS.f20, '99', 3), 84);
assert.equal(measureText(FONTS.f20, '100', 2), 84);
assert.match(formatEpoch(1789805806), /^\d{2}-\d{2} \d{2}:\d{2}$/);
assert.match(formatEpoch(1789561987, 'hhmm'), /^\d{2}:\d{2}$/);

const blockPixels = (raster, x, y, w, h) => {
  let count = 0;
  for (let row = y; row < y + h; row += 1) {
    for (let column = x; column < x + w; column += 1) count += raster.pixels[row * WIDTH + column] === BLACK ? 1 : 0;
  }
  return count;
};
assert.ok(blockPixels(normal, 4, 4, 104, 90) > 104 * 90 / 2, 'weekly block is present');
assert.ok(blockPixels(normal, 92, 106, 104, 90) > 104 * 90 / 2, '5h block is present');

const countColor = (raster, x, y, w, h, color) => {
  let count = 0;
  for (let row = y; row < y + h; row += 1) {
    for (let column = x; column < x + w; column += 1) count += raster.pixels[row * WIDTH + column] === color ? 1 : 0;
  }
  return count;
};
assert.ok(countColor(normal, 112, 46, 84, 18, 0) > 0 && countColor(normal, 112, 46, 84, 18, 255) > 0, 'metadata text has foreground and background');

const fiveHourBinding = parseBind('buckets[codex].5h.remaining');
const different = (first, second) => first.pixels.some((pixel, index) => pixel !== second.pixels[index]);
assert.ok(different(normal, full), '100% fixture changes visible hero pixels');
assert.ok(different(normal, missingFiveHour), 'missing 5h fixture changes the hero to static 100');
assert.equal(bindResult(fiveHourBinding, usageFixture({ fiveHourUsed: 100 }), env).value, 0, '100% used means 0% remaining');
assert.ok(countColor(used100, 4, 158, 84, 12, 0) > 0, 'present 5h window shows its reset time');
assert.equal(countColor(missingFiveHour, 4, 158, 84, 12, 0), 0, 'missing 5h hides its reset time');
assert.deepEqual(render(template, usageFixture({ fiveHourUsed: 11 }), env).pixels, normal.pixels, 'same fixture is deterministic');
assert.ok(different(normal, render(template, usageFixture({ fiveHourUsed: 11 }), { ...env, battery: 77 })), 'battery change changes visible pixels');
assert.ok(different(normal, render(template, usageFixture({ fiveHourUsed: 11 }), { ...env, state: 'BLE ON' })), 'state change changes visible pixels');

const rcBind = parseBind('resetCredits.availableCount');
assert.equal(bindResult(rcBind, usageFixture(), env).exists, false, 'missing reset credits hide the RC line');
const withReset = render(template, usageFixture({ fiveHourUsed: 11, resetCount: 2 }), env);
assert.equal(bindResult(rcBind, usageFixture({ resetCount: 2 }), env).value, 2, 'reset credit count is readable');
assert.ok(different(normal, withReset), 'reset credits change visible pixels');
assert.equal(countColor(render(template, usageFixture({ fiveHourUsed: 11, label: null }), env), 112, 64, 84, 18, 0), 0, 'missing username hides the label row');
const labelElement = template.elements.find((element) => element.bind === 'bridge.label');
assert.ok(labelElement && !labelElement.prefix, 'bridge label has no LABEL prefix');

const hostTemplate = validateTemplate({
  schema: 1,
  canvas: { w: 200, h: 200 },
  elements: [{ type: 'text', bind: 'bridge.hostId', region: [0, 0, 100, 12], font: 'f12' }],
});
const hostA = usageFixture({ fiveHourUsed: 11 });
const hostB = usageFixture({ fiveHourUsed: 11 });
hostB.bridge.hostId = 'other';
assert.ok(different(render(hostTemplate, hostA, env), render(hostTemplate, hostB, env)), 'hostId bind changes visible pixels');

const sparkOnly = usageFixture({ weeklyUsed: 91, fiveHourUsed: null });
sparkOnly.buckets = sparkOnly.buckets.filter(({ id }) => id === 'codex');
assert.deepEqual(render(template, sparkOnly, env).pixels, missingFiveHour.pixels, 'Spark 5h is never borrowed');
assert.ok(different(longNames, normal), 'long plan and label fixture is rendered');

assert.deepEqual(bindResult(fiveHourBinding, usageFixture({ fiveHourUsed: 0 }), env), { exists: true, value: 100 }, '0% used is a present 5h window');
assert.equal(bindResult(fiveHourBinding, usageFixture({ fiveHourUsed: null }), env).exists, false, 'missing 5h selects static 100');
assert.equal(template.elements.filter((element) => element.type === 'text' && element.text === '100').length, 1, 'missing 5h has static 100 hero');

const stateBind = parseBind('device.state');
const offlineBind = parseBind('device.offline_mins');
assert.equal(bindResult(stateBind, usageFixture(), env).value, 'BLE OFF', 'state bind renders the wire state');
assert.equal(bindResult(stateBind, usageFixture(), { ...env, state: '' }).exists, false, 'empty state hides');
assert.equal(bindResult(stateBind, usageFixture(), { ...env, state: 'WIFI OFF' }).value, 'WIFI OFF', 'WIFI OFF state renders');
assert.equal(bindResult(offlineBind, usageFixture(), offlineEnv).value, 138, 'offline minutes bind renders its value');
assert.equal(bindResult(offlineBind, usageFixture(), env).exists, false, 'unknown offline minutes hide');
assert.equal(bindResult(offlineBind, usageFixture(), { ...offlineEnv, offline_mins: -1 }).exists, false, 'negative offline minutes hide');
assert.equal(bindResult(offlineBind, usageFixture(), { ...offlineEnv, offline_mins: 0 }).exists, true, 'zero is a valid value (device policy gates bridge-loss display)');
const modeBind = parseBind('device.mode');
assert.equal(modeBind?.kind, 'device.mode', 'device.mode is a known bind');
assert.equal(bindResult(modeBind, usageFixture(), env).value, 'light', 'preview defaults to light mode');
assert.equal(bindResult(modeBind, usageFixture(), { ...env, mode: 'deep' }).value, 'deep', 'deep mode renders its value');
const deep = render(template, usageFixture({ fiveHourUsed: 11 }), { ...env, state: 'DEEP', mode: 'deep' });
assert.ok(different(normal, deep), 'deep mode changes visible pixels');
assert.ok(countColor(deep, 101, 6, 16, 16, 0) > 0, 'deep mode draws the sleep glyph in the BT cell');
assert.equal(countColor(normal, 101, 6, 16, 16, 0), 0, 'light mode leaves the BT cell empty (BLE off)');
assert.ok(countColor(normal, 4, 158, 84, 12, 0) > 0, '5H reset row sits at y=158');
assert.ok(countColor(normal, 4, 172, 84, 12, 0) > 0, 'BATT row sits at y=172');
assert.ok(countColor(normal, 4, 186, 84, 12, 0) > 0, 'online SYNC row sits at y=186');
assert.ok(countColor(offline, 4, 186, 84, 12, 0) > 0, 'OFF row shows when the bridge is unreachable');
const syncElement = template.elements.find((element) => element.bind === 'device.sync_hhmm');
assert.deepEqual(syncElement?.when, { bind: 'device.offline_mins', exists: false }, 'SYNC row is gated on the bridge-unreachable bind');
const regionSlice = (raster, x, y, w, h) => {
  const out = [];
  for (let row = y; row < y + h; row += 1) {
    for (let col = x; col < x + w; col += 1) out.push(raster.pixels[row * WIDTH + col]);
  }
  return out;
};
assert.notDeepEqual(regionSlice(offline, 4, 186, 84, 12), regionSlice(normal, 4, 186, 84, 12), 'offline OFF row replaces the online SYNC row');

const monthlyBind = parseBind('buckets[codex].monthly.remaining');
assert.equal(bindResult(monthlyBind, monthlyUsage, env).exists, true, 'monthly window is classified by duration');
assert.equal(bindResult(monthlyBind, usageFixture({ fiveHourUsed: 11 }), env).exists, false, 'weekly-only plans have no monthly window');
const weekLabel = template.elements.find((element) => element.text === 'WEEK');
const monthLabel = template.elements.find((element) => element.text === 'MONTH');
assert.deepEqual(weekLabel?.when, { bind: 'buckets[codex].monthly.remaining', exists: false }, 'WEEK label is gated on the missing monthly window');
assert.deepEqual(monthLabel?.when, { bind: 'buckets[codex].monthly.remaining', exists: true }, 'MONTH label is gated on the monthly window');
assert.notDeepEqual(regionSlice(monthly, 8, 78, 88, 18), regionSlice(normal, 8, 78, 88, 18), 'monthly plan replaces the WEEK label with MONTH');
assert.deepEqual(regionSlice(missingFiveHour, 8, 78, 88, 18), regionSlice(normal, 8, 78, 88, 18), 'weekly plans keep the WEEK label without a 5h window');
assert.ok(different(normal, offline), 'offline screen differs from the normal screen');
assert.ok(different(offline, offlineMissing), 'missing 5h changes the offline screen');
assert.deepEqual(render(template, usageFixture({ fiveHourUsed: 11 }), offlineEnv).pixels, offline.pixels, 'offline fixture is deterministic');

assert.ok(fs.existsSync(new URL('test-bridge/templates/quad.json', import.meta.url)));
console.log('quad preview tests: 7 fixtures, bounds/palette/fit/date/conditions/RC/label/state/offline/determinism passed');
