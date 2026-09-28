import net from 'node:net';
import http from 'node:http';
import {appendFileSync, mkdirSync} from 'node:fs';
import {dirname} from 'node:path';
import {Decoder, encode, lrc} from './codec.mjs';
import {Terminal, reply} from './model.mjs';

export async function serve({port = 3000, controlPort = 13001, token, wirePath, runId = 'manual'} = {}) {
  if (typeof token !== 'string' || token.length < 16) throw Error('SSI_CONTROL_TOKEN must contain at least 16 characters');
  if (!wirePath) throw Error('A wire journal path is required');
  mkdirSync(dirname(wirePath), {recursive: true});
  const epoch = performance.now(), events = [], sockets = new Set();
  let nextConnection = 0, cursor = 0, context = {run_id: runId, scenario_id: 'unarmed'};
  const emit = (kind, data = {}) => {
    const event = {seq: ++cursor, utc: new Date().toISOString(), elapsed_ms: performance.now() - epoch,
      ...context, kind, ...data};
    // The file is authoritative. Fail closed on write failure; never silently lose evidence.
    appendFileSync(wirePath, JSON.stringify(event) + '\n');
    events.push(event); if (events.length > 10000) events.shift();
  };
  const terminal = new Terminal(emit);
  const tick = setInterval(() => terminal.tick(), 10);
  const tcp = net.createServer(socket => {
    const connection_id = ++nextConnection, decoder = new Decoder();
    const log = (kind, data) => emit(kind, {connection_id, ...data});
    if (sockets.size >= 32) { socket.destroy(); return; }
    sockets.add(socket); socket.setTimeout(20000);
    log('connected', {peer: socket.remoteAddress});
    socket.on('timeout', () => {
      // Only incomplete inbound frames expire. Silence faults must measure the
      // PayLink timeout, not an artificial responder-side disconnect deadline.
      if (decoder.pending.length) { log('incomplete_frame_timeout', {pending_hex: decoder.pending.toString('hex')}); socket.destroy(); }
    });
    socket.on('error', e => log('socket_error', {error: e.message}));
    socket.on('close', () => {
      sockets.delete(socket); log('disconnected', {pending_hex: decoder.pending.toString('hex')});
    });
    let chain = Promise.resolve();
    socket.on('data', bytes => {
      log('bytes', {direction: 'rx', hex: bytes.toString('hex')});
      for (const frame of decoder.push(bytes)) {
        const generation = terminal.generation;
        chain = chain.then(async () => {
          if (socket.destroyed || generation !== terminal.generation) return;
          log('frame', {direction: 'rx', ...frame});
          if (frame.fatal) { socket.destroy(); return; }
          const result = frame.error ? reply('Unknown', frame.error) : terminal.handle(frame.json);
          const s = structuredClone(terminal.active?.scenario ?? (['GetLastResult', 'GetResultByUid'].includes(result.method) ? terminal.last?.scenario : null) ?? terminal.armed);
          const fault = s.faults[result.method] ?? 'normal';
          log('response_prepared', {json: result, fault});
          if (fault === 'silence') return;
          if (fault === 'close') { socket.destroy(); return; }
          if (fault === 'unknown_status') result.status = 'S99';
          let bytes = encode(result);
          if (fault === 'bad_lrc') bytes[bytes.length - 1] ^= 1;
          if (fault === 'bad_length') bytes.writeUInt16BE(bytes.length + 20, 3);
          if (fault === 'bad_json') {
            bytes[5] = 0x21; bytes[bytes.length - 1] = lrc(bytes.subarray(5, -1));
          }
          const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
          if (s.response_delay_ms) await pause(s.response_delay_ms);
          if (generation !== terminal.generation || socket.destroyed) return;
          const decoded = new Decoder().push(bytes)[0];
          log('frame', {direction: 'tx', hex: bytes.toString('hex'), json: decoded?.json ?? null,
            ...(decoded?.error || !decoded ? {error: decoded?.error ?? 'incomplete_length'} : {})});
          const size = s.fragment_bytes || bytes.length;
          for (let at = 0; at < bytes.length; at += size) {
            if (generation !== terminal.generation || socket.destroyed) return;
            const chunk = bytes.subarray(at, at + size);
            await new Promise((resolve, reject) => socket.write(chunk, e => e ? reject(e) : resolve()));
            log('bytes', {direction: 'tx', hex: chunk.toString('hex')});
            if (at + size < bytes.length && s.fragment_delay_ms) await pause(s.fragment_delay_ms);
          }
          log('response_written', {method: result.method});
        }).catch(e => { log('delivery_failed', {error: e.message}); socket.destroy(); });
      }
    });
  });
  const control = http.createServer(async (req, res) => {
    const send = (status, body) => { res.writeHead(status, {'Content-Type': 'application/json'}); res.end(JSON.stringify(body)); };
    if (req.headers.origin || req.headers.authorization !== `Bearer ${token}`) return send(403, {error: 'forbidden'});
    const url = new URL(req.url, 'http://localhost');
    try {
      if (req.method === 'GET') {
        if (url.pathname === '/state') return send(200, terminal.state());
        if (url.pathname === '/health') return send(200, {ready: true, run_id: context.run_id});
        if (url.pathname === '/events') {
          const after = Number(url.searchParams.get('after') ?? 0);
          return send(200, {first_seq: events[0]?.seq, last_seq: cursor, truncated: after < (events[0]?.seq ?? 1) - 1,
            events: events.filter(e => e.seq > after)});
        }
      }
      if (req.method !== 'POST' || !['/arm', '/reset'].includes(url.pathname)) return send(404, {error: 'unknown route'});
      let body = '';
      for await (const chunk of req) { body += chunk; if (Buffer.byteLength(body) > 65536) return send(413, {error: 'body too large'}); }
      const value = JSON.parse(body || '{}');
      if (url.pathname === '/arm') {
        terminal.arm(value);
        context = {...context, scenario_id: terminal.armed.id};
      } else {
        if (Object.keys(value).some(k => !['preserve_result', 'run_id'].includes(k)) ||
          (value.preserve_result !== undefined && typeof value.preserve_result !== 'boolean') ||
          (value.run_id !== undefined && !/^[\w-]{1,100}$/.test(value.run_id))) throw Error('Invalid reset');
        for (const s of sockets) s.destroy();
        context = {run_id: value.run_id ?? context.run_id, scenario_id: 'unarmed'};
        terminal.reset(value);
      }
      send(200, terminal.state());
    } catch (e) { send(400, {error: e.message}); }
  });
  control.requestTimeout = 5000;
  const listen = (server, p) => new Promise((resolve, reject) => {
    server.once('error', reject); server.listen(p, '127.0.0.1', () => { server.off('error', reject); resolve(); });
  });
  try { await listen(tcp, port); await listen(control, controlPort); }
  catch (e) { clearInterval(tick); tcp.close(); control.close(); throw e; }
  const ready = {tcp: `127.0.0.1:${tcp.address().port}`, control_url: `http://127.0.0.1:${control.address().port}`, wire_path: wirePath};
  emit('ready', ready);
  return {ready, terminal, close: async () => {
    clearInterval(tick);
    const closed = [...sockets].map(s => new Promise(r => { s.once('close', r); s.destroy(); }));
    control.closeAllConnections();
    await Promise.all([new Promise(r => tcp.close(r)), new Promise(r => control.close(r))]);
    await Promise.all(closed);
  }};
}
