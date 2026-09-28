// Replay reviewed, sanitized real PayLink recordings against the HTTP emulator.
import {readFileSync, writeFileSync, mkdirSync, readdirSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {recordHttp, control as command} from './lab/http.mjs';
import {compare} from './lab/compare.mjs';
const requireReference = process.argv.includes('--require-reference');
const fixtureDir = 'profiles/desktop-paylink-2.1.20-win-x86/reference';
const files = existsSync(fixtureDir) ? readdirSync(fixtureDir).filter(f => f.endsWith('.json')) : [];
mkdirSync('test-results', {recursive: true});
if (!files.length) {
  writeFileSync('test-results/differential.json', JSON.stringify({status: requireReference ? 'blocked' : 'not_run', required: requireReference, reason: 'No sanitized PayLink 2.1.20 reference recordings', verified: 0, skipped: 0, mismatched: 0}, null, 2));
  console.error('No reference recordings; this is not a compatibility pass'); process.exit(requireReference ? 2 : 0);
}
const payment = process.env.PAYLINK_PAYMENT_URL || 'http://127.0.0.1:3000';
const control = process.env.PAYLINK_CONTROL_URL || 'http://127.0.0.1:3001';
const token = process.env.PAYLINK_CONTROL_TOKEN;
if (!token) throw Error('PAYLINK_CONTROL_TOKEN required');
const results = [];
for (const file of files) {
  try {
    const f = JSON.parse(readFileSync(join(fixtureDir, file)));
    if (f.profile !== 'desktop-paylink-2.1.20-win-x86' || !f.evidence?.bank || !f.evidence?.protocol || !f.evidence?.capture_date || f.evidence?.installer_sha256 !== '62d0e7a539380937ecb5af2f1c50438e4b84a070de4a20c1c848c11a5e395491') throw Error('Incomplete reference provenance');
    if (f.evidence.provenance === 'real_paylink_simulated_ssi_json') {
      for (const key of ['executable_sha256', 'ssi_sha256', 'ssi_revision', 'scenario_id', 'raw_trace', 'raw_trace_sha256']) if (!f.evidence[key]) throw Error(`Missing ${key}`);
      const path = f.evidence.raw_trace;
      if (path.includes('..') || !path.startsWith('evidence/')) throw Error('Trace must be inside reference/evidence');
      if (createHash('sha256').update(readFileSync(join(fixtureDir, path))).digest('hex') !== f.evidence.raw_trace_sha256) throw Error('Raw evidence hash mismatch');
    }
    if (f.comparison_status === 'unsupported') {
      if (!f.reason) throw Error('Unsupported case requires a reason');
      results.push({file, status: 'unsupported', reason: f.reason}); continue;
    }
    if (!f.request.path.startsWith('/api/') || f.request.path.includes('://')) throw Error('Fixture path must be local /api/...');
    await command(control, token, 'reset', {}, true);
    if (f.scenario) await command(control, token, 'arm', f.scenario, true);
    const actual = await recordHttp(payment + f.request.path, f.request, {timeoutMs: f.client_timeout_ms ?? 30000});
    const expected = {status: f.response.status, headers: {'content-type': f.response.content_type ?? 'application/json'},
      body: f.response.body ?? null, body_text: f.response.body_text ?? JSON.stringify(f.response.body),
      connection_error: f.response.connection_error ?? null, duration_ms: f.response.duration_ms};
    results.push({file, ...compare(expected, actual, f)});
  } catch (e) { results.push({file, status: 'error', error: e.message}); }
}
const summary = {verified: results.filter(r => r.status === 'verified').length,
  mismatched: results.filter(r => r.status === 'mismatch').length,
  skipped: results.filter(r => r.status === 'unsupported').length,
  errors: results.filter(r => r.status === 'error').length};
const status = summary.mismatched || summary.errors ? 'failed' : summary.skipped ? 'partial' : 'verified';
writeFileSync('test-results/differential.json', JSON.stringify({profile: 'desktop-paylink-2.1.20-win-x86', status, ...summary, results}, null, 2));
console.log(JSON.stringify({status, ...summary}));
if (summary.mismatched || summary.errors) process.exitCode = 1;
else if (requireReference && (!summary.verified || summary.skipped)) process.exitCode = 2;
