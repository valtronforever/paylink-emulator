import http from 'node:http';
import https from 'node:https';
import {appendFileSync} from 'node:fs';
// Unlike fetch, retain raw header pairs, non-JSON bodies, partial bodies and errors.
export function recordHttp(url, request, {file, runId, scenarioId, timeoutMs = 30000, epoch = performance.now()} = {}) {
  const target = new URL(url);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(target.hostname)) throw Error('Lab HTTP targets must be loopback');
  if (target.username || target.password) throw Error('Credentials in URL are not permitted');
  const start = performance.now();
  const rawBody = request.body === undefined ? undefined : JSON.stringify(request.body);
  const headers = {...request.headers, ...(rawBody === undefined ? {} : {'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(rawBody)})};
  const entry = {run_id: runId, scenario_id: scenarioId, start_utc: new Date().toISOString(),
    start_ms: start - epoch, client_timeout_ms: timeoutMs, request: {url, method: request.method, headers, body: rawBody ?? null}};
  return new Promise(resolve => {
    const chunks = []; let finished = false, timer;
    const finish = error => {
      if (finished) return; finished = true; clearTimeout(timer);
      entry.end_utc = new Date().toISOString(); entry.duration_ms = performance.now() - start;
      entry.body_hex = Buffer.concat(chunks).toString('hex'); entry.body_text = Buffer.concat(chunks).toString('utf8');
      try { entry.body = JSON.parse(entry.body_text); } catch { entry.body = null; }
      entry.connection_error = error ?? null;
      if (file) appendFileSync(file, JSON.stringify(entry) + '\n');
      resolve(entry);
    };
    const client = (target.protocol === 'https:' ? https : http).request(target, {method: request.method, headers}, response => {
      entry.status = response.statusCode; entry.headers = response.headers; entry.raw_headers = response.rawHeaders;
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => finish());
      response.on('error', e => finish(e.message));
      response.on('aborted', () => finish('response_aborted'));
    });
    client.on('error', e => finish(e.code ?? e.message));
    // Total deadline, not socket inactivity timeout.
    timer = setTimeout(() => { finish('client_timeout'); client.destroy(); }, timeoutMs);
    client.end(rawBody);
  });
}

export async function control(url, token, resource, body, emulator = false) {
  const target = new URL(url);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(target.hostname)) throw Error('Control must be loopback');
  const response = await fetch(`${url}${emulator ? '/control/v1' : ''}/${resource}`, {
    method: body === undefined ? 'GET' : 'POST',
    headers: {Authorization: `Bearer ${token}`, 'Content-Type': 'application/json'},
    ...(body === undefined ? {} : {body: JSON.stringify(emulator ? {command_id: crypto.randomUUID(), payload: body} : body)}),
    signal: AbortSignal.timeout(10000),
  });
  if (!response.ok) throw Error(`${resource}: HTTP ${response.status} ${await response.text()}`);
  return response.json();
}
