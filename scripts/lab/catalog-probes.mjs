// Controlled injections, not claims that arbitrary result/code combinations
// can occur on a physical terminal. Preserve every chosen combination.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {join} from 'node:path';
import {randomUUID,createHash} from 'node:crypto';
import {serve} from '../ssi/server.mjs';
import {inventory,spec} from '../ssi/inventory.mjs';
import {recordHttp,control} from './http.mjs';
const config=JSON.parse(readFileSync(process.argv[2],'utf8')),root=process.argv[3];
if(!root)throw Error('Usage: catalog-probes.mjs config.json new-output-directory');
const exeHash=createHash('sha256').update(readFileSync(config.executable_path)).digest('hex');
if(exeHash!=='85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465')throw Error('Unexpected PayLink build');
const schema=await recordHttp(config.reference_url+'/swagger/v1/swagger.json',{method:'GET'});
if(!schema.body?.paths?.['/api/pos/devices/{device_id}']?.delete)throw Error('Device cleanup route not documented');
mkdirSync(root,{recursive:false});
const cases=[...inventory.states.filter(r=>r.code!=='S00').map(r=>({id:r.code,phases:[{status:r.code,ms:3000}]})),
  ...inventory.financial_results.map(r=>({id:r.code,transaction_result:r.code,
    error_code:/^(APPROVED|COMPLETED|OK)/.test(r.code)?'':'E21',response_code:/^(APPROVED|COMPLETED|OK)/.test(r.code)?'00':'51'}))];
let next=0;const results=[];
await Promise.all(Array.from({length:4},async()=>{
  while(next<cases.length){
    const scenario=cases[next++],id=randomUUID(),token=randomUUID(),wire=join(root,`${scenario.id}-ssi.jsonl`);
    const sim=await serve({port:0,controlPort:0,token,wirePath:wire,runId:scenario.id});
    const device=structuredClone(config.device);device.id=id;device.name=`catalog-${scenario.id}`;device.properties.port=Number(sim.ready.tcp.split(':')[1]);
    const options={file:join(root,'http.jsonl'),scenarioId:scenario.id};
    try{
      const registration=await recordHttp(config.reference_url+'/api/pos/devices/register',{method:'POST',body:device},{...options,stepId:'register'});
      if(registration.status!==200)throw Error('Registration failed');
      await control(sim.ready.control_url,token,'arm',scenario);
      const response=await recordHttp(config.reference_url+`/api/pos/${id}/purchase`,{method:'POST',body:{amount:100,merchant_id:'TEST-MERCHANT'}},{...options,stepId:'purchase',timeoutMs:30000});
      const events=readFileSync(wire,'utf8').trim().split('\n').map(JSON.parse);
      const sent=events.filter(e=>e.kind==='frame'&&e.direction==='tx').map(e=>e.json);
      results.push({id:scenario.id,scenario,device,runtime:sim.ready,status:response.status,body:response.body,
        duration_ms:response.duration_ms,connection_error:response.connection_error,ssi_responses:sent});
      console.log(JSON.stringify({id:scenario.id,status:response.status,code:response.body?.code,duration_ms:response.duration_ms}));
    }catch(e){results.push({id:scenario.id,error:e.message});}
    finally{
      await recordHttp(config.reference_url+`/api/pos/devices/${id}`,{method:'DELETE'},{...options,stepId:'cleanup'});
      await sim.close();
    }
  }
}));
writeFileSync(join(root,'catalog.json'),JSON.stringify({spec,executable_sha256:exeHash,
  scope:'Synthetic state/result probes; explicit code/result combinations are test inputs, not a normative financial mapping',results},null,2));
if(results.some(r=>r.error||r.connection_error))process.exitCode=1;
