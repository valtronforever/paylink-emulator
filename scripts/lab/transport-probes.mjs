// Each fault uses a separate synthetic device; never alter another device.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {serve} from '../ssi/server.mjs';
import {spec} from '../ssi/inventory.mjs';
import {recordHttp,control} from './http.mjs';
const config=JSON.parse(readFileSync(process.argv[2],'utf8')),root=process.argv[3];
if(!root)throw Error('Usage: transport-probes.mjs config.json new-output-directory');
const executableSha256=createHash('sha256').update(readFileSync(config.executable_path)).digest('hex');
if(executableSha256!=='85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465')throw Error('Unexpected PayLink build');
const schema=await recordHttp(config.reference_url+'/swagger/v1/swagger.json',{method:'GET'});
if(!schema.body?.paths?.['/api/pos/devices/{device_id}']?.delete)throw Error('Device cleanup route not documented');
mkdirSync(root,{recursive:false});
const cases=['close','bad_lrc','bad_length','bad_json','unknown_status'].map(fault=>({id:fault,faults:{Purchase:fault}}));
cases.push({id:'close-result',faults:{GetLastResult:'close'}});
const results=await Promise.all(cases.map(async scenario=>{
  const id=randomUUID(),token=randomUUID(),wire=join(root,`${scenario.id}-ssi.jsonl`);
  const sim=await serve({port:0,controlPort:0,token,wirePath:wire,runId:scenario.id});
  const device=structuredClone(config.device);device.id=id;device.name=`transport-${scenario.id}`;device.properties.port=Number(sim.ready.tcp.split(':')[1]);
  const options={file:join(root,'http.jsonl'),scenarioId:scenario.id};
  try{
    const registered=await recordHttp(config.reference_url+'/api/pos/devices/register',{method:'POST',body:device},{...options,stepId:'register'});
    if(registered.status!==200)throw Error('Registration failed');
    await control(sim.ready.control_url,token,'arm',scenario);
    const response=await recordHttp(config.reference_url+`/api/pos/${id}/purchase`,{method:'POST',body:{amount:100,merchant_id:'TEST-MERCHANT'}},{...options,stepId:'purchase',timeoutMs:210000});
    let recovery=null;
    if(response.connection_error!=='client_timeout'){
      await control(sim.ready.control_url,token,'arm',{id:scenario.id+'-recovery'});
      recovery=await recordHttp(config.reference_url+`/api/pos/${id}/ping`,{method:'GET'},{...options,stepId:'recovery-ping'});
    }
    const row={id:scenario.id,scenario,device,runtime:sim.ready,status:response.status,body:response.body,duration_ms:response.duration_ms,connection_error:response.connection_error,recovery_status:recovery?.status,terminal:sim.terminal.state()};
    console.log(JSON.stringify({id:row.id,status:row.status,duration_ms:row.duration_ms,recovery_status:row.recovery_status}));return row;
  }finally{
    await recordHttp(config.reference_url+`/api/pos/devices/${id}`,{method:'DELETE'},{...options,stepId:'cleanup'});
    await sim.close();
  }
}));
writeFileSync(join(root,'transport.json'),JSON.stringify({spec,executable_sha256:executableSha256,scope:'Synthetic malformed response and disconnect probes; no physical reachability claim',results},null,2));
if(results.some(r=>r.connection_error))process.exitCode=1;
