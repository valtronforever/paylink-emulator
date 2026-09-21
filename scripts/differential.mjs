// Compare sanitized reference recordings against a running emulator.
// Never connects to a bank or records a real purchase itself.
import {readFileSync,writeFileSync,mkdirSync,readdirSync,existsSync} from 'node:fs';
import {join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {normalizeReference as normalized} from './normalize-reference.mjs';
const requireReference=process.argv.includes('--require-reference');
const fixtureDir='profiles/desktop-paylink-2.1.20-win-x86/reference';
const files=existsSync(fixtureDir)?readdirSync(fixtureDir).filter(f=>f.endsWith('.json')):[];
mkdirSync('test-results',{recursive:true});
if(!files.length){
  writeFileSync('test-results/differential.json',JSON.stringify({status:requireReference?'blocked':'not_run',required:requireReference,reason:'No sanitized PayLink 2.1.20 reference recordings',verified:0},null,2));
  console.error(requireReference?'Blocked: reference fixtures required by --require-reference':'Not run: optional reference comparison has no recordings; documentation-based validation is independent');process.exit(requireReference?2:0);
}
const payment=process.env.PAYLINK_PAYMENT_URL||'http://127.0.0.1:3000';
const control=process.env.PAYLINK_CONTROL_URL||'http://127.0.0.1:3001';
const token=process.env.PAYLINK_CONTROL_TOKEN;if(!token)throw new Error('PAYLINK_CONTROL_TOKEN required');
const command=async(resource,payload)=>{
  const r=await fetch(`${control}/control/v1/${resource}`,{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:JSON.stringify({command_id:randomUUID(),payload})});
  if(!r.ok)throw new Error(await r.text());return r.json();
};
const results=[];
for(const file of files){
  const f=JSON.parse(readFileSync(join(fixtureDir,file)));
  if(f.profile!=='desktop-paylink-2.1.20-win-x86'||!f.evidence?.bank||!f.evidence?.protocol||!f.evidence?.capture_date||f.evidence?.installer_sha256!=='62d0e7a539380937ecb5af2f1c50438e4b84a070de4a20c1c848c11a5e395491')throw new Error(`Incomplete version/bank/protocol evidence: ${file}`);
  if(!f.request.path.startsWith('/api/')||f.request.path.includes('://'))throw new Error('Fixture path must be local /api/...');
  await command('reset',{});if(f.scenario)await command('arm',f.scenario);
  const start=performance.now();const r=await fetch(payment+f.request.path,{method:f.request.method,headers:f.request.headers,...(f.request.body?{body:JSON.stringify(f.request.body)}:{})});
  const body=await r.json();const elapsed=performance.now()-start;
  const status=r.status===f.response.status;
  const payload=isDeepStrictEqual(normalized(body,f.variable_fields||[]),normalized(f.response.body,f.variable_fields||[]));
  const timing=!f.timing||elapsed>=f.timing.min_ms&&elapsed<=f.timing.max_ms;
  results.push({file,status:status&&payload&&timing?'pass':'failed',http_status:status,payload,timing,elapsed_ms:elapsed});
}
writeFileSync('test-results/differential.json',JSON.stringify({profile:'desktop-paylink-2.1.20-win-x86',results},null,2));
if(results.some(r=>r.status!=='pass'))process.exitCode=1;
