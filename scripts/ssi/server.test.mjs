import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import net from 'node:net';
import {serve} from './server.mjs';
import {encode, Decoder} from './codec.mjs';
test('durable completed result survives responder restart; accepted work clears stale result', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'ssi-persist-'));
  const options = {port:0,controlPort:0,token:'isolated-test-token',wirePath:join(dir,'wire.jsonl'),statePath:join(dir,'state.json')};
  let server = await serve(options);
  const request = {method:'Purchase',params:{transAmount:'100',transCurrency:'980',merchantId:'TEST-MERCHANT',transactionUid:'durable-test'}};
  try {
    server.terminal.arm({id:'instant',phases:[]}); server.terminal.handle(request); server.terminal.tick();
    await server.close(); server = await serve(options);
    assert.equal(server.terminal.handle({method:'GetResultByUid',params:{transactionUid:'durable-test'}}).params.transAmount, '100');
    server.terminal.arm({id:'pending',phases:[{status:'S02',ms:10000}]}); server.terminal.handle(request);
    await server.close(); server = await serve(options);
    assert.equal(server.terminal.state().last, null); assert.equal(server.terminal.state().status,'S00');
  } finally { await server.close(); rmSync(dir,{recursive:true,force:true}); }
});
test('real TCP fragmentation, coalescing, control auth and durable wire evidence', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'ssi-test-')), token = 'isolated-test-token';
  const server = await serve({port: 0, controlPort: 0, token, wirePath: join(dir, 'wire.jsonl')});
  try {
    assert.equal((await fetch(server.ready.control_url + '/state')).status, 403);
    assert.equal((await fetch(server.ready.control_url + '/state', {headers: {Authorization: `Bearer ${token}`, Origin: 'https://example.com'}})).status, 403);
    const socket = net.connect(Number(server.ready.tcp.split(':')[1]), '127.0.0.1');
    const response = new Promise((resolve, reject) => {
      const decoder = new Decoder(), frames = [];
      const timer = setTimeout(() => reject(Error('TCP response deadline')), 3000);
      socket.on('data', chunk => { frames.push(...decoder.push(chunk)); if (frames.length === 2) { clearTimeout(timer); resolve(frames); } });
      socket.on('error', reject);
    });
    const request = encode({method: 'PingDevice'});
    socket.write(request.subarray(0, 2)); socket.write(Buffer.concat([request.subarray(2), encode({method: 'Unknown'})]));
    const replies = await response;
    assert.deepEqual(replies.map(r => r.json.errorCode), ['', 'E05']);
    socket.destroy();
    const events = readFileSync(join(dir, 'wire.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
    assert.ok(events.some(e => e.kind === 'unknown_method'));
    assert.ok(events.some(e => e.direction === 'rx' && e.hex));
    assert.ok(events.some(e => e.direction === 'tx' && e.hex));
  } finally { await server.close(); rmSync(dir, {recursive: true, force: true}); }
});
