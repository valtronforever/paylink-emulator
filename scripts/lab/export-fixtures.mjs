// Explicit local export, after inspecting a run for synthetic data only.
// Never uploads files or copies native logs, certificates, DBs or control headers.
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
const root = process.argv[2];
if (!root || !process.argv.includes('--reviewed-synthetic')) throw Error('Usage: export-fixtures.mjs <run-dir> --reviewed-synthetic');
const manifest = JSON.parse(readFileSync(join(root, 'manifest.json')));
if (manifest.status !== 'recorded') throw Error('Only completed runs can be exported');
const records = readFileSync(join(root, 'reference/http.jsonl'), 'utf8').trim().split('\n').map(JSON.parse).filter(e => e.scenario_id !== 'preflight');
const wire = readFileSync(join(root, 'reference/ssi-wire.jsonl'), 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse);
const dir = 'profiles/desktop-paylink-2.1.20-win-x86/reference';
mkdirSync(join(dir, 'evidence'), {recursive: true});
for (const record of records) {
  const id = record.scenario_id;
  const scenario = JSON.parse(readFileSync(join(root, 'scenarios', `${id}.json`)));
  if (scenario.requests) throw Error('Concurrent groups require sequence-aware review; do not export as isolated fixtures');
  if (record.connection_error) throw Error(`${id}: connection failure requires manual fixture review`);
  const raw = {run_id: manifest.run_id, scenario, http: record, ssi: wire.filter(e => e.scenario_id === id),
    build: {commit: manifest.commit, dirty: manifest.dirty, executable_sha256: manifest.executable_sha256, installer_sha256: manifest.installer_sha256, ssi: manifest.ssi, host: manifest.host}};
  const serialized = JSON.stringify(raw, null, 2), rawPath = `evidence/${manifest.run_id}-${id}.json`;
  if (/authorization|cookie|private key|track2/i.test(serialized)) throw Error('Sensitive-key screening failed; inspect and sanitize manually');
  writeFileSync(join(dir, rawPath), serialized);
  const referenceError = /^E\d\d$/.test(id) ? id : id === 'declined' ? 'E21' : id === 'timeout' ? 'transport_timeout' : null;
  const supported = ['ping', 'approved'].includes(id) || !!referenceError;
  const fixture = {profile: manifest.profile, evidence: {
    provenance: manifest.provenance, installer_sha256: manifest.installer_sha256,
    executable_sha256: manifest.executable_sha256, bank: manifest.device_configuration.bank_name,
    protocol: 'SSIJson', capture_date: record.start_utc.slice(0,10), ssi_revision: manifest.ssi.revision,
    ssi_sha256: manifest.ssi.sha256, scenario_id: id, raw_trace: rawPath,
    raw_trace_sha256: createHash('sha256').update(serialized).digest('hex'),
  }, request: scenario.request, scenario: referenceError ? (id === 'timeout' ? {id, outcome:'approved', reference_error:referenceError} : {id, outcome: 'error', error_id: 'terminal_general', reference_error: referenceError}) : scenario.emulator,
    response: {status: record.status, content_type: record.headers['content-type'], body: record.body, duration_ms: record.duration_ms},
    variable_fields: scenario.variable_fields ?? [], id_mapping: scenario.id_mapping ?? [],
    ...(!supported ? {comparison_status: 'unsupported', reason: 'SSI-specific error/result mapping is not represented by the current documentation-based emulator scenario; see SSI findings.'} : {})};
  writeFileSync(join(dir, `${id}.json`), JSON.stringify(fixture, null, 2));
}
console.log(`Exported ${records.length} reviewed reference fixtures`);
