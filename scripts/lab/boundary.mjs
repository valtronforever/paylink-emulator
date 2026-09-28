// Separate simulated devices allow independent boundary probes on one PayLink.
// Every device is explicitly registered as SSIJson + loopback; no bank traffic.
import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {randomUUID, createHash} from 'node:crypto';
import {serve} from '../ssi/server.mjs';
import {recordHttp, control} from './http.mjs';
import {spec} from '../ssi/inventory.mjs';
const config = JSON.parse(readFileSync(process.argv[2], 'utf8')), root = process.argv[3];
if (!root) throw Error('Usage: boundary.mjs config.json new-output-directory');
const executableSha256=createHash('sha256').update(readFileSync(config.executable_path)).digest('hex');
if(executableSha256!=='85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465')throw Error('Unexpected PayLink build');
mkdirSync(root, {recursive:false});
const results = [];
const ports = [], epoch = performance.now();
const schema = await recordHttp(config.reference_url + '/swagger/v1/swagger.json', {method:'GET'});
if (!schema.body?.paths?.['/api/pos/devices/{device_id}']?.delete) throw Error('Device cleanup route not documented');
try {
  for (let repeat = 1; repeat <= 2; repeat++) {
    await Promise.all([144000,146000].map(async delay => {
      const id = randomUUID(), caseId = `delay-${delay}-repeat-${repeat}`, token = randomUUID();
      const wire = join(root, `${caseId}-ssi.jsonl`), file = join(root, 'http.jsonl');
      const simulator = await serve({port:0,controlPort:0,token,wirePath:wire,runId:caseId});
      const device = structuredClone(config.device);
      device.id=id; device.name=caseId; device.properties.port=Number(simulator.ready.tcp.split(':')[1]);
      ports.push({case_id:caseId,device,simulator:simulator.ready});
      const scenario = {id:caseId,method_delays:{Purchase:delay}};
      try {
        const registered = await recordHttp(config.reference_url + '/api/pos/devices/register', {method:'POST',body:device}, {file,scenarioId:caseId,stepId:'register',epoch});
        if (registered.status !== 200) throw Error('Test device registration failed');
        await control(simulator.ready.control_url,token,'arm',scenario);
        const response = await recordHttp(config.reference_url + `/api/pos/${id}/purchase`, {method:'POST',body:{amount:100,merchant_id:'TEST-MERCHANT'}}, {file,scenarioId:caseId,stepId:'purchase',timeoutMs:210000,epoch});
        const events=readFileSync(wire,'utf8').trim().split('\n').map(JSON.parse);
        const methods=events.filter(e=>e.kind==='frame'&&e.direction==='rx').map(e=>e.json?.method);
        const row={case_id:caseId,scenario,http_status:response.status,success:response.body?.success,error:response.connection_error,
          duration_ms:response.duration_ms,recovery:methods.includes('GetResultByUid'),methods,terminal:simulator.terminal.state()};
        results.push(row);console.log(JSON.stringify({case_id:caseId,status:response.status,duration_ms:response.duration_ms,recovery:row.recovery}));
      } finally {
        await recordHttp(config.reference_url + `/api/pos/devices/${id}`, {method:'DELETE'}, {file,scenarioId:caseId,stepId:'cleanup',epoch});
        await simulator.close();
      }
    }));
  }
} finally {
  const groups=[144000,146000].map(delay=>{
    const values=results.filter(r=>r.scenario.method_delays.Purchase===delay).map(r=>r.duration_ms);
    return {delay_ms:delay,samples:values,min_ms:Math.min(...values),max_ms:Math.max(...values),range_ms:Math.max(...values)-Math.min(...values)};
  });
  writeFileSync(join(root,'boundary.json'),JSON.stringify({scope:'Real PayLink with synthetic delayed acknowledgements; observed bracket, not an exact timeout guarantee',
    spec,executable_sha256:executableSha256,executable_sha256_after:createHash('sha256').update(readFileSync(config.executable_path)).digest('hex'),ports,groups,results},null,2));
}
