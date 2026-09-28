// Standalone CLI harness. Browser security stays enabled and payment traffic is real.
import {chromium} from '@playwright/test';
import http from 'node:http';
import https from 'node:https';
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {control} from './http.mjs';
const config = JSON.parse(readFileSync(process.argv[2] ?? 'examples/ssi/lab.json', 'utf8'));
const out = process.argv[3]; if (!out) throw Error('Usage: node scripts/lab/browser.mjs config.json output-directory');
mkdirSync(out, {recursive: false});
const handler = (_req, res) => {
  res.writeHead(200, {'Content-Type': 'text/html; charset=utf-8'});
  res.end('<!doctype html><html><meta charset="utf-8"><title>SSI lab browser evidence</title><h1>PayLink SSI lab</h1><pre id="result">Ready</pre></html>');
};
const server = config.browser_tls ? https.createServer({key: readFileSync(config.browser_tls.key), cert: readFileSync(config.browser_tls.cert)}, handler) : http.createServer(handler);
await new Promise(r => server.listen(13020, '127.0.0.1', r));
const origin = config.browser_origin ?? `${config.browser_tls ? 'https' : 'http'}://127.0.0.1:13020`;
const browser = await chromium.launch({channel: config.browser_channel ?? 'chromium', headless: true});
const summary = [];
try {
  for (const target of ['reference', 'emulator']) {
    if (!config[`${target}_url`]) continue;
    const context = await browser.newContext(); // No ignoreHTTPSErrors or disable-web-security.
    await context.tracing.start({screenshots: true, snapshots: true, sources: true});
    const page = await context.newPage(), events = [];
    page.on('console', e => events.push({kind: 'console', utc: new Date().toISOString(), type: e.type(), text: e.text()}));
    page.on('pageerror', e => events.push({kind: 'page_error', error: e.message}));
    page.on('requestfailed', r => events.push({kind: 'request_failed', url: r.url(), error: r.failure()}));
    page.on('response', r => events.push({kind: 'response', url: r.url(), status: r.status(), headers: r.headers()}));
    await page.goto(origin);
    for (const item of config.scenarios) {
      if (target === 'reference') {
        await control(config.ssi_control_url, process.env.SSI_CONTROL_TOKEN, 'reset', {run_id: `browser-${target}`});
        await control(config.ssi_control_url, process.env.SSI_CONTROL_TOKEN, 'arm', {...item.ssi, id: item.id});
      } else {
        if (!item.emulator) continue;
        await control(config.emulator_control_url, process.env.PAYLINK_CONTROL_TOKEN, 'reset', {}, true);
        await control(config.emulator_control_url, process.env.PAYLINK_CONTROL_TOKEN, 'arm', item.emulator, true);
      }
      const result = await page.evaluate(async ({url, request, timeout}) => {
        const start = performance.now(); let result;
        try {
          const r = await fetch(url, {method: request.method, headers: request.body ? {'Content-Type': 'application/json'} : {},
            ...(request.body ? {body: JSON.stringify(request.body)} : {}), signal: AbortSignal.timeout(timeout)});
          result = {status: r.status, body: await r.text(), elapsed_ms: performance.now() - start};
        } catch (e) { result = {error: e.message, elapsed_ms: performance.now() - start}; }
        document.querySelector('#result').textContent = JSON.stringify(result, null, 2); return result;
      }, {url: config[`${target}_url`] + (target === 'emulator' ? item.emulator_path ?? item.request.path : item.request.path), request: item.request, timeout: item.timeout_ms ?? 30000});
      summary.push({target, scenario_id: item.id, origin, ...result});
      await page.screenshot({path: join(out, `${target}-${item.id}.png`)});
      if (result.error && !result.error.includes('fetch')) break;
    }
    writeFileSync(join(out, `${target}-events.json`), JSON.stringify(events, null, 2));
    await context.tracing.stop({path: join(out, `${target}-trace.zip`)}); await context.close();
  }
} finally {
  writeFileSync(join(out, 'summary.json'), JSON.stringify({origin, browser: browser.version(), inerix: 'not_run_dependency_476', https_to_localhost: origin.startsWith('https:') ? 'see_results' : 'not_run_no_trusted_harness_certificate', results: summary}, null, 2));
  await browser.close(); await new Promise(r => server.close(r));
}
