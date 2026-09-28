import {readFileSync, writeFileSync} from 'node:fs';
import {inventory} from '../ssi/inventory.mjs';
const config = JSON.parse(readFileSync(process.argv[2] ?? 'examples/ssi/lab.json', 'utf8'));
delete config.emulator_url;
const purchase = config.scenarios.find(s => s.id === 'approved').request;
config.scenarios = inventory.errors.map(row => ({id: row.code, request: purchase,
  ssi: row.category === 'request' ? {request_errors: {Purchase: row.code}} : {error_code: row.code, response_code: '', transaction_result: 'FAILED'},
  timeout_ms: 30000,
}));
writeFileSync(process.argv[3] ?? '.runtime/ssi-lab/matrix.json', JSON.stringify(config, null, 2));
