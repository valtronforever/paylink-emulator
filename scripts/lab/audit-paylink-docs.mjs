// Audit the native PayLink HTTP contract, not the emulator or SSI error mapping.
// All writes target temporary devices connected only to an owned loopback responder.
import fs from 'node:fs';
import {join} from 'node:path';
import {createHash, randomUUID} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {serve} from '../ssi/server.mjs';
import {recordHttp, control} from './http.mjs';
const config=JSON.parse(fs.readFileSync(process.argv[2],'utf8')),out=process.argv[3];
const quickOnly=process.argv.includes('--quick');
if(!out)throw Error('Usage: audit-paylink-docs.mjs lab-config.json new-output-directory [--quick]');
const hash=p=>createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const expected='85d1bf9280e7c63e1099ecc2db480813f439e68c41a5eca8a453094a948ab465';
if(hash(config.executable_path)!==expected)throw Error('Unexpected POSServer build');
fs.mkdirSync(out,{recursive:false});
fs.copyFileSync('scripts/lab/audit-paylink-docs.mjs',join(out,'source.mjs'));
const runId=`paylink-docs-${randomUUID()}`,epoch=performance.now(),file=join(out,'http.jsonl');
const manifest={run_id:runId,started_utc:new Date().toISOString(),scope:'Native PayLink documentation audit with synthetic loopback devices',
  executable_sha256:expected,commit:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),
  source_sha256:hash('scripts/lab/audit-paylink-docs.mjs'),working_tree_dirty:!!execFileSync('git',['status','--porcelain'],{encoding:'utf8'}).trim(),
  quick_only:quickOnly,reference_url:config.reference_url,cases:[],devices:[],status:'running'};
const save=()=>fs.writeFileSync(join(out,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
save();
const request=async(id,path,method='GET',body,extra={})=>{
  const r=await recordHttp(config.reference_url+path,{method,...(body===undefined?{}:{body}),...extra.request},
    {file,runId,scenarioId:id,epoch,timeoutMs:extra.timeoutMs??30000});
  manifest.cases.push({id,path,method,status:r.status,duration_ms:r.duration_ms,connection_error:r.connection_error});save();
  console.log(JSON.stringify({id,status:r.status,duration_ms:r.duration_ms,body:r.body}));return r;
};
const schema=await recordHttp(config.reference_url+'/swagger/v1/swagger.json',{method:'GET'});
if(schema.status!==200||!schema.body?.paths?.['/api/pos/devices/{device_id}']?.delete)throw Error('Local API schema/cleanup unavailable');
fs.writeFileSync(join(out,'openapi.json'),schema.body_text);
manifest.openapi_sha256=hash(join(out,'openapi.json'));
const sessions=[];
async function device(label,registrationCode){
  const id=randomUUID(),token=randomUUID();
  const sim=await serve({port:0,controlPort:0,token,wirePath:join(out,`${label}-ssi.jsonl`),runId});
  const dto=structuredClone(config.device);Object.assign(dto,{id,name:`DOC-AUDIT-${label}`,code_registration:registrationCode});
  dto.properties={...dto.properties,host:'127.0.0.1',port:Number(sim.ready.tcp.split(':')[1]),resolve_ip_by_mac:false};
  const session={id,sim,token,label};sessions.push(session);manifest.devices.push({label,configuration:dto,runtime:sim.ready});save();
  if((await request(`register-${label}`,'/api/pos/devices/register','POST',dto)).status!==200)throw Error('Registration failed');
  return session;
}
const arm=(s,value)=>control(s.sim.ready.control_url,s.token,'arm',value);
try{
  const quick=await device('contract','DOC-AUDIT-A');
  await device('unassigned','');
  const slow=quickOnly?null:await device('timeout','DOC-AUDIT-B');
  await Promise.all([
    (async()=>{
      const missing=randomUUID();
      for(const [id,path] of [
        ['devices','/api/devices'],['pos-devices','/api/pos/devices'],
        ['device',`/api/pos/devices/${quick.id}`],['missing-device',`/api/pos/devices/${missing}`],
        ['missing-ping',`/api/pos/${missing}/ping`],['filter-A','/api/devices?CodeRegistration=DOC-AUDIT-A'],
        ['filter-missing','/api/devices?CodeRegistration=DOC-AUDIT-NONE'],['filter-empty','/api/devices?CodeRegistration=']])await request(id,path);
      const route=`/api/pos/${quick.id}/purchase`;
      for(const [id,body] of [['missing-amount',{}],['negative-amount',{amount:-1}],['overflow-amount',{amount:4294967296}],
        ['approved',{amount:100,merchant_id:'TEST-MERCHANT'}],['unknown-property',{amount:101,merchant_id:'TEST-MERCHANT',audit_unknown:true}],
        ['string-amount',{amount:'102',merchant_id:'TEST-MERCHANT'}]]){
        await arm(quick,{id});await request(id,route,'POST',body);
      }
      await arm(quick,{id:'general-error',request_errors:{Purchase:'E00'}});
      await request('general-error',route,'POST',{amount:100,merchant_id:'TEST-MERCHANT'});
      await request('cors-ping',`/api/pos/${quick.id}/ping`,'GET',undefined,{request:{headers:{Origin:'https://paylink-audit.example'}}});
      await request('cors-preflight',route,'OPTIONS',undefined,{request:{headers:{Origin:'https://paylink-audit.example','Access-Control-Request-Method':'POST','Access-Control-Request-Headers':'content-type'}}});
      await arm(quick,{id:'busy',phases:[{status:'S02',ms:3500}]});
      await Promise.all([
        request('busy-owner',route,'POST',{amount:103,merchant_id:'TEST-MERCHANT'}),
        (async()=>{await new Promise(r=>setTimeout(r,300));await request('busy-ping',`/api/pos/${quick.id}/ping`);})(),
      ]);
      await request('last-result',`/api/pos/${quick.id}/last-result`);
    })(),
    (async()=>{
      if(!slow)return;
      const scenario={id:'wiki-timeout',faults:{Purchase:'silence'},request_errors:{GetResultByUid:'E20'}};
      manifest.timeout_scenario=scenario;await arm(slow,scenario);
      const response=await request('wiki-timeout',`/api/pos/${slow.id}/purchase`,'POST',{amount:100,merchant_id:'TEST-MERCHANT'},{timeoutMs:210000});
      fs.writeFileSync(join(out,'timeout-state.json'),JSON.stringify(slow.sim.terminal.state(),null,2));
      if(response.connection_error==='client_timeout')throw Error('Native operation unresolved; do not infer cancellation');
      await request('timeout-recovery-ping',`/api/pos/${slow.id}/ping`);
    })(),
  ]);
  manifest.status='recorded';
}catch(e){manifest.status='failed';manifest.failure=e.message;process.exitCode=1;}
finally{
  for(const s of sessions){
    await request(`cleanup-${s.label}`,`/api/pos/devices/${s.id}`,'DELETE');
    await s.sim.close();
  }
  manifest.finished_utc=new Date().toISOString();manifest.executable_sha256_after=hash(config.executable_path);
  if(manifest.executable_sha256_after!==expected){manifest.status='invalid_build_changed';process.exitCode=1;}save();
}
