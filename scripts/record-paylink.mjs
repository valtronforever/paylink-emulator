import {readFileSync, writeFileSync, mkdirSync, copyFileSync, existsSync} from 'node:fs';
import {join, resolve} from 'node:path';
import {createHash, randomUUID} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import os from 'node:os';
import {parseArgs} from 'node:util';
import {recordHttp, control} from './lab/http.mjs';
import {compare} from './lab/compare.mjs';
const {values} = parseArgs({options: {config: {type: 'string'}, out: {type: 'string'}}});
if (!values.config) throw Error('--config <lab.json> required; see docs/WINDOWS_SSI_LAB.md');
const config = JSON.parse(readFileSync(values.config, 'utf8'));
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const pinned = JSON.parse(readFileSync('profiles/desktop-paylink-2.1.20-win-x86/manifest.json'));
if (hash(config.installer_path) !== pinned.installer.sha256) throw Error('Installer hash mismatch');
if (hash(config.executable_path) !== '85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465') throw Error('POSServer hash mismatch');
if (hash(config.spec_path) !== '8e4b459ade28f4ee301de7751659d4bf365f9c0f2ac3508a6745a597158e1a65') throw Error('SSI specification hash mismatch');
if (!Array.isArray(config.scenarios) || !config.scenarios.length) throw Error('Nonempty scenarios required');
if (new Set(config.scenarios.map(s => s.id)).size !== config.scenarios.length || config.scenarios.some(s => !/^[\w-]{1,80}$/.test(s.id))) throw Error('Scenario IDs must be unique safe filenames');
const runId = `${new Date().toISOString().replace(/[:.]/g, '-')}-${randomUUID().slice(0, 8)}`;
const root = resolve(values.out ?? join('runs', runId));
if (existsSync(root)) throw Error('Run directory already exists; never overwrite evidence');
for (const dir of ['reference/paylink-logs', 'emulator', 'browser', 'scenarios']) mkdirSync(join(root, dir), {recursive: true});
const manifest = {run_id: runId, started_utc: new Date().toISOString(), provenance: 'real_paylink_simulated_ssi_json',
  profile: pinned.id, installer_sha256: hash(config.installer_path), executable_sha256: hash(config.executable_path),
  ssi: {revision: '1.4.6', date: '2026-09-12', wire_version: 1, sha256: hash(config.spec_path)},
  commit: execFileSync('git', ['rev-parse', 'HEAD'], {encoding: 'utf8'}).trim(),
  dirty: !!execFileSync('git', ['status', '--porcelain'], {encoding: 'utf8'}).trim(),
  host: {platform: os.platform(), release: os.release(), version: os.version(), architecture: os.arch(), node: process.version},
  reference_url: config.reference_url, emulator_url: config.emulator_url ?? null,
  device_configuration: config.device, id_mapping: config.id_mapping ?? {},
  native_logs: [], missing_evidence: [], verified_banks: [], status: 'running'};
writeFileSync(join(root, 'manifest.json'), JSON.stringify(manifest, null, 2));
const results = [], epoch = performance.now();
const ssi = (r, b) => control(config.ssi_control_url, process.env.SSI_CONTROL_TOKEN, r, b);
const emu = (r, b) => control(config.emulator_control_url, process.env.PAYLINK_CONTROL_TOKEN, r, b, true);
try {
  // Readiness is an actual control roundtrip. POSServer readiness/API schema is recorded below.
  await ssi('health');
  const schema = await recordHttp(config.reference_url + '/swagger/v1/swagger.json', {method: 'GET'}, {file: join(root, 'reference/http.jsonl'), runId, scenarioId: 'preflight', epoch});
  if (schema.status !== 200 || !schema.body?.paths) throw Error('PayLink local OpenAPI unavailable');
  writeFileSync(join(root, 'reference/openapi.json'), JSON.stringify(schema.body, null, 2));
  for (const item of config.scenarios) {
    writeFileSync(join(root, 'scenarios', `${item.id}.json`), JSON.stringify(item, null, 2));
    await ssi('reset', {run_id: runId}); await ssi('arm', {...item.ssi, id: item.id});
    const reference = await recordHttp(config.reference_url + item.request.path, item.request, {
      file: join(root, 'reference/http.jsonl'), runId, scenarioId: item.id, timeoutMs: item.timeout_ms ?? 30000, epoch});
    const state = await ssi('state');
    writeFileSync(join(root, 'reference', `${item.id}-state.json`), JSON.stringify(state, null, 2));
    let result = {scenario_id: item.id, status: 'not_run', reason: 'Emulator not configured', reference_status: reference.status, reference_error: reference.connection_error};
    if (config.emulator_url && item.emulator) {
      await emu('reset', {}); await emu('arm', item.emulator);
      const request = {...item.request, path: item.emulator_path ?? item.request.path};
      const candidate = await recordHttp(config.emulator_url + request.path, request, {
        file: join(root, 'emulator/http.jsonl'), runId, scenarioId: item.id, timeoutMs: item.timeout_ms ?? 30000, epoch});
      result = {scenario_id: item.id, ...compare(reference, candidate, item)};
      writeFileSync(join(root, 'emulator', `${item.id}-journal.json`), JSON.stringify(await emu('journal'), null, 2));
    }
    results.push(result); console.log(JSON.stringify({...result, reference_ms: reference.duration_ms}));
    // Client timeout is not cancellation. Refuse to reset away an unresolved operation.
    if (reference.connection_error === 'client_timeout') {
      manifest.missing_evidence.push(`${item.id}: client timed out; PayLink may still be busy. Run recovery separately.`);
      break;
    }
  }
  manifest.status = 'recorded';
} catch (e) { manifest.status = 'failed'; manifest.failure = e.message; process.exitCode = 1; }
finally {
  // Filter shared append-only wire journal by out-of-band run ID, preserving bytes.
  if (existsSync(config.wire_path)) {
    const lines = readFileSync(config.wire_path, 'utf8').split('\n').filter(Boolean).filter(line => JSON.parse(line).run_id === runId);
    writeFileSync(join(root, 'reference/ssi-wire.jsonl'), lines.join('\n') + '\n');
  } else manifest.missing_evidence.push('SSI wire journal missing');
  for (const file of config.native_logs ?? []) {
    const target = `native-${manifest.native_logs.length}.log`;
    try { copyFileSync(file, join(root, 'reference/paylink-logs', target)); manifest.native_logs.push({source: file, file: target, sha256: hash(file)}); }
    catch (e) { manifest.missing_evidence.push(`Native log unavailable: ${e.code}`); }
  }
  if (!manifest.native_logs.length) manifest.missing_evidence.push('Native logs were not supplied');
  manifest.missing_evidence.push('Database consistent export not captured', 'Browser recordings are a separate stage');
  manifest.finished_utc = new Date().toISOString();
  manifest.executable_sha256_after = hash(config.executable_path);
  if (manifest.executable_sha256_after !== manifest.executable_sha256) { manifest.status = 'invalid_build_changed'; process.exitCode = 1; }
  const summary = Object.fromEntries(['verified', 'mismatch', 'not_run'].map(s => [s, results.filter(r => r.status === s).length]));
  summary.not_run += config.scenarios.length - results.length;
  if (summary.mismatch) process.exitCode = 1;
  writeFileSync(join(root, 'manifest.json'), JSON.stringify(manifest, null, 2));
  writeFileSync(join(root, 'comparison.json'), JSON.stringify({summary, results}, null, 2));
  writeFileSync(join(root, 'REPORT.md'), `# PayLink + simulated SSI JSON\n\nRun: ${runId}\n\nStatus: ${manifest.status}\n\n${JSON.stringify(summary)}\n\nPhysical terminal, EMV and bank certification: unverified.\n\n${results.map(r => `- ${r.scenario_id}: ${r.status}`).join('\n')}\n\nMissing evidence:\n${manifest.missing_evidence.map(s => `- ${s}`).join('\n')}\n`);
  console.log(JSON.stringify({run_directory: root, status: manifest.status, summary}));
}
