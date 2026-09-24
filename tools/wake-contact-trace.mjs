#!/usr/bin/env node
// Correlate device format-2 /history rows with authenticated Bridge BLE logs.
// This reports observed stages only; it deliberately does not infer a root cause.

import assert from 'node:assert/strict';
import fs from 'node:fs/promises';

function option(args, name, fallback = undefined) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : fallback;
}

function integer(value) {
  if (Number.isInteger(value)) return value;
  if (typeof value === 'string' && /^\d+$/.test(value)) return Number(value);
  return null;
}

function mac(value) {
  const normalized = String(value || '').replace(/[^0-9a-f]/gi, '').toUpperCase();
  return normalized.length === 12 ? normalized : null;
}

function identity(value) {
  const generation = integer(value?.wake_generation);
  const seq = integer(value?.wake_seq ?? value?.seq);
  return generation === null || seq === null ? null : `${generation}:${seq}`;
}

function parseLogLine(line) {
  const fields = {};
  const fieldPattern = /([A-Za-z_][A-Za-z0-9_]*)=(?:"((?:\\.|[^"])*)"|(\S+))/g;
  for (const match of line.matchAll(fieldPattern)) {
    fields[match[1]] = match[2] === undefined ? match[3] : match[2].replace(/\\(["\\])/g, '$1');
  }
  const timestamp = line.match(/^(\S+)/)?.[1] || '';
  const message = line.replace(fieldPattern, '').replace(/^\S+\s+/, '').trim();
  const deviceMac = mac(fields.device_mac || fields.verified_mac);
  const kind = fields.wake_association === 'uncertain'
    ? 'candidate'
    : fields.event === 'wake_contact_summary'
      ? 'failure'
      : message.includes('BLE command acknowledged')
        ? 'ack'
        : message.includes('BLE command written')
          ? 'write'
          : fields.wake_association === 'confirmed'
            ? 'confirmed'
            : fields.stage || message.includes('BLE stage')
              ? 'stage'
              : 'other';
  return { timestamp, fields, message, deviceMac, kind, identity: identity(fields), line };
}

function historyRows(value) {
  if (Array.isArray(value)) return value;
  if (Array.isArray(value?.records)) return value.records;
  if (Array.isArray(value?.history)) return value.history;
  return [];
}

function stage(row) {
  return row?.device_last_stage || row?.wake_stage || row?.last_stage || row?.stage || 'unknown';
}

function relatedEvidence(row, events) {
  const rowIdentity = identity(row);
  const exact = events.filter((event) => event.identity === rowIdentity && event.kind !== 'candidate');
  const candidates = events.filter((event) => event.kind === 'candidate');
  const confirmed = exact.filter((event) => event.kind === 'confirmed');
  const writes = exact.filter((event) => event.kind === 'write');
  const acks = exact.filter((event) => event.kind === 'ack');
  const failures = exact.filter((event) => event.kind === 'failure');
  const bridgeLast = exact.at(-1);
  let breakpoint;
  if (!confirmed.length) {
    breakpoint = candidates.length
      ? 'Bridge 仅记录到未认证扫描候选，未记录同一醒次的 MAC + wake_generation + wake_seq 确认'
      : '设备记录存在，但 Bridge 日志没有候选或已认证会合记录';
  } else if (failures.length) {
    breakpoint = `Bridge 记录了失败摘要：${failures.at(-1).fields.bridge_last_stage || failures.at(-1).fields.result || 'unknown'}`;
  } else if (acks.length) {
    breakpoint = 'Bridge 记录了与该醒次匹配的 ACK';
  } else if (writes.length) {
    breakpoint = 'Bridge 记录了命令写入，但没有与该醒次匹配的 ACK';
  } else {
    breakpoint = 'Bridge 记录了 MAC 与醒次确认，但没有命令写入或 ACK 记录';
  }
  return { rowIdentity, exact, confirmed, writes, acks, failures, bridgeLast, breakpoint };
}

function formatRow(row, evidence) {
  const transport = row.transport || row.wake_transport || 'unknown';
  const result = row.result || 'unknown';
  const duration = integer(row.awake_ms);
  const bridge = evidence.bridgeLast
    ? `${evidence.bridgeLast.kind}/${evidence.bridgeLast.fields.stage || evidence.bridgeLast.fields.bridge_last_stage || evidence.bridgeLast.fields.result || 'observed'}`
    : 'none';
  return `  gen=${row.wake_generation} seq=${row.wake_seq ?? row.seq} type=${row.wake_type || 'unknown'} result=${result} stage=${stage(row)} awake_ms=${duration ?? 'unknown'} transport=${transport} bridge=${bridge}`;
}

export function analyze(history, logText, expectedMac = null) {
  const rows = historyRows(history);
  const allEvents = logText
    .split(/\r?\n/)
    .filter(Boolean)
    .map(parseLogLine)
    .filter((event) => !expectedMac || event.deviceMac === expectedMac);
  const format2 = rows.filter((row) => row?.format === 2 && identity(row));
  const ignored = rows.length - format2.length;
  return { rows, format2, ignored, events: allEvents, candidates: allEvents.filter((event) => event.kind === 'candidate') };
}

async function fetchHistory(ip, since, limit) {
  const params = new URLSearchParams();
  if (since !== undefined) params.set('since', since);
  if (limit !== undefined) params.set('limit', limit);
  const query = params.size ? `?${params}` : '';
  const response = await fetch(`http://${ip}/history${query}`, { signal: AbortSignal.timeout(10000) });
  if (!response.ok) throw new Error(`/history returned HTTP ${response.status}`);
  return response.json();
}

async function selfTest() {
  const report = analyze(
    [{ format: 2, wake_generation: 3, seq: 8, wake_type: 'timer', result: 'answered', awake_ms: 1200 }],
    '2026-09-24T01:00:00Z INFO bridge_ble: BLE CodexStatus advertisement device_mac=70041DD7A340 wake_association="uncertain"\n'
      + '2026-09-24T01:00:01Z INFO bridge_ble: BLE device identity verified device_mac=70041DD7A340 wake_generation=3 wake_seq=8 wake_association="confirmed"\n'
      + '2026-09-24T01:00:02Z INFO bridge_ble: BLE command written device_mac=70041DD7A340 request_id="r-full"\n'
      + '2026-09-24T01:00:03Z INFO bridge_ble: BLE command acknowledged device_mac=70041DD7A340 request_id="r-full" wake_generation=3 wake_seq=8 wake_association="confirmed"',
    '70041DD7A340',
  );
  assert.equal(report.format2.length, 1);
  const evidence = relatedEvidence(report.format2[0], report.events);
  assert.equal(evidence.confirmed.length, 1);
  assert.equal(evidence.acks.length, 1);
  assert.match(evidence.breakpoint, /ACK/);
  console.log('wake-contact-trace self-test: format-2 identity, candidate uncertainty, request_id, and ACK correlation passed');
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('--self-test')) return selfTest();
  const ip = option(args, '--ip', args.find((arg) => !arg.startsWith('--')) || '192.168.3.163');
  const logPath = option(args, '--log');
  const expectedMac = mac(option(args, '--mac'));
  const since = option(args, '--since');
  const limit = option(args, '--limit');
  if (!logPath) throw new Error('usage: node tools/wake-contact-trace.mjs --log <bridge-app.log> [--ip <device-ip>] [--mac <wifi-mac>] [--since <seq>] [--limit <n>]');
  const [history, logText] = await Promise.all([fetchHistory(ip, since, limit), fs.readFile(logPath, 'utf8')]);
  const report = analyze(history, logText, expectedMac);
  console.log(`wake-contact-trace device=${expectedMac || 'all'} ip=${ip}`);
  console.log(`history rows=${report.rows.length} format2=${report.format2.length} ignored_legacy_or_invalid=${report.ignored}`);
  console.log(`bridge events=${report.events.length} candidates_uncertain=${report.candidates.length}`);
  for (const row of report.format2) {
    const evidence = relatedEvidence(row, report.events);
    console.log(formatRow(row, evidence));
    console.log(`    evidence: ${evidence.breakpoint}`);
    if (evidence.exact.length) {
      const requestIds = [...new Set(evidence.exact.map((event) => event.fields.request_id).filter(Boolean))];
      if (requestIds.length) console.log(`    request_id: ${requestIds.join(', ')}`);
      console.log(`    bridge_stages: ${evidence.exact.map((event) => event.fields.stage || event.kind).join(' -> ')}`);
    }
  }
  const orphanConfirmed = report.events.filter((event) => event.kind === 'confirmed' && !report.format2.some((row) => identity(row) === event.identity));
  if (orphanConfirmed.length) console.log(`unmatched_authenticated_events=${orphanConfirmed.length} (history may be from another RTC generation or was overwritten)`);
}

try {
  await main();
} catch (error) {
  console.error(`wake-contact-trace: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
