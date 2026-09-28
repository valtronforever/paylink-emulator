import {test} from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import {recordHttp} from './http.mjs';
import {compare} from './compare.mjs';
test('recorder retains HTTP errors, raw headers and non-JSON; total timeout terminates', async () => {
  const server = http.createServer((req, res) => {
    if (req.url === '/hang') return;
    res.writeHead(503, {'Content-Type': 'text/plain', 'X-Test': 'retained'}); res.end('declined');
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}`;
  try {
    const entry = await recordHttp(base, {method: 'GET'});
    assert.equal(entry.status, 503); assert.equal(entry.body_text, 'declined'); assert.equal(entry.body, null);
    assert.ok(entry.raw_headers.includes('retained'));
    const timeout = await recordHttp(base + '/hang', {method: 'GET'}, {timeoutMs: 30});
    assert.equal(timeout.connection_error, 'client_timeout');
    assert.equal(compare(entry, {...entry, status: 200}).status, 'mismatch');
    assert.equal(compare(entry, {...entry, headers: {'content-type': 'application/json'}}).status, 'mismatch');
    assert.throws(() => recordHttp('https://example.com', {method: 'GET'}));
  } finally { server.closeAllConnections(); await new Promise(r => server.close(r)); }
});
