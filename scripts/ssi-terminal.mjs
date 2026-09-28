import {parseArgs} from 'node:util';
import {readFileSync} from 'node:fs';
import {serve} from './ssi/server.mjs';
const {values, positionals} = parseArgs({allowPositionals: true, options: {
  port: {type: 'string', default: '3000'}, 'control-port': {type: 'string', default: '13001'},
  wire: {type: 'string', default: 'runs/manual/reference/ssi-wire.jsonl'},
  file: {type: 'string'}, 'run-id': {type: 'string', default: 'manual'},
}});
const token = process.env.SSI_CONTROL_TOKEN;
const command = positionals[0] ?? 'serve';
if (command === 'serve') {
  const server = await serve({port: Number(values.port), controlPort: Number(values['control-port']),
    token, wirePath: values.wire, runId: values['run-id']});
  console.log(JSON.stringify(server.ready));
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, async () => { await server.close(); process.exit(0); });
} else {
  if (!['arm', 'state', 'events', 'reset'].includes(command)) throw Error('Expected serve, arm, state, events, reset');
  const body = values.file ? JSON.parse(readFileSync(values.file, 'utf8')) : {};
  const response = await fetch(`${process.env.SSI_CONTROL_URL ?? 'http://127.0.0.1:13001'}/${command}`, {
    method: ['arm', 'reset'].includes(command) ? 'POST' : 'GET',
    headers: {Authorization: `Bearer ${token}`, 'Content-Type': 'application/json'},
    ...(['arm', 'reset'].includes(command) ? {body: JSON.stringify(body)} : {}),
    signal: AbortSignal.timeout(5000),
  });
  console.log(await response.text()); if (!response.ok) process.exitCode = 1;
}
