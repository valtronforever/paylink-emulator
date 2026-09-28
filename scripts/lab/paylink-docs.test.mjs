// These checks validate recorded native evidence, not fresh native execution.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {resolve,sep} from 'node:path';
import {createHash} from 'node:crypto';
const root=resolve('profiles/desktop-paylink-2.1.20-win-x86/reference/paylink-doc-audit');
const read=p=>JSON.parse(fs.readFileSync(resolve(root,p),'utf8'));
const lines=p=>fs.readFileSync(resolve(root,p),'utf8').trim().split('\n').map(JSON.parse);
const hash=p=>createHash('sha256').update(fs.readFileSync(resolve(root,p))).digest('hex');
test('native documentation audit preserves build, source, raw bytes and cleanup evidence',()=>{
  const index=read('index.json');
  for(const file of index.files){
    assert.ok(resolve(root,file.path).startsWith(root+sep));
    assert.equal(hash(file.path),file.sha256,file.path);
  }
  for(const run of ['full','repeat']){
    const manifest=read(`${run}/manifest.json`),records=lines(`${run}/http.jsonl`);
    assert.equal(manifest.status,'recorded');
    assert.equal(manifest.executable_sha256,index.executable_sha256);
    assert.equal(manifest.executable_sha256_after,index.executable_sha256);
    assert.equal(hash(`${run}/source.mjs`),manifest.source_sha256);
    assert.equal(hash(`${run}/openapi.json`),manifest.openapi_sha256);
    assert.equal(records.length,24);
    for(const record of records){
      assert.equal(record.connection_error,null,record.scenario_id);
      assert.equal(Buffer.from(record.body_hex,'hex').toString('utf8'),record.body_text);
      assert.ok(!Object.keys(record.request.headers).some(k=>/authorization|cookie/i.test(k)));
    }
    for(const device of manifest.devices){
      assert.equal(device.configuration.properties.host,'127.0.0.1');
      assert.equal(records.find(r=>r.scenario_id===`cleanup-${device.label}`).status,200);
    }
  }
});
test('seven reported PayLink documentation gaps are supported by native observations',()=>{
  const full=lines('full/http.jsonl'),repeat=lines('repeat/http.jsonl');
  const timeout=full.find(r=>r.scenario_id==='wiki-timeout');
  assert.equal(timeout.status,503);assert.equal(timeout.body.code,9016);assert.ok(timeout.duration_ms>120000);
  const wire=lines('full/timeout-ssi.jsonl');
  const requests=wire.filter(e=>e.kind==='frame'&&e.direction==='rx');
  const purchase=requests.find(e=>e.json?.method==='Purchase');
  const lookup=requests.find(e=>e.json?.method==='GetResultByUid');
  assert.ok(lookup.elapsed_ms-purchase.elapsed_ms>120000);
  assert.equal(requests.filter(e=>e.json?.method==='GetResultByUid').length,8);
  assert.ok(wire.some(e=>e.kind==='completed'&&e.error_code===''));
  for(const [run,records] of [['full',full],['repeat',repeat]]){
    const schema=read(`${run}/openapi.json`),s=schema.components.schemas;
    const row=id=>records.find(r=>r.scenario_id===id);
    assert.equal(schema.paths['/api/devices'].get.responses['200'].content['application/json'].schema.$ref,'#/components/schemas/DeviceDTO');
    assert.equal(s.DeviceDTO.type,'object');assert.ok(Array.isArray(row('devices').body));
    assert.equal(s.TerminalStatuses.type,'integer');assert.equal(row('approved').body.terminal_status,'None');
    assert.ok(!s.ProtocolType.enum.includes(row('device').body.protocol));
    assert.ok(!s.DeviceCategory.enum.includes(row('device').body.device_category));
    assert.ok(s.DeviceDTO.required.includes('BankName'));assert.ok(!('BankName' in row('device').body));
    assert.ok('bank_name' in row('device').body);
    assert.equal(schema.paths['/api/pos/{device_id}/purchase'].post.responses['400'].content['application/json'].schema.$ref,'#/components/schemas/HTTPValidationError');
    assert.equal(row('missing-amount').status,400);assert.ok(!('detail' in row('missing-amount').body));assert.ok('msg' in row('missing-amount').body);
    assert.equal(schema.paths['/api/pos/devices/{device_id}'].get.responses['404'].content['application/json'].schema.$ref,'#/components/schemas/ErrorResponse');
    assert.equal(row('missing-device').status,404);assert.ok(!('error' in row('missing-device').body));
    assert.deepEqual(Object.keys(schema.paths['/api/pos/{device_id}/ping'].get.responses),['200']);
    assert.equal(row('missing-ping').status,404);assert.equal(row('busy-ping').status,503);
  }
  const blank=read('repeat/manifest.json').devices.find(d=>d.label==='unassigned').configuration;
  assert.equal(blank.code_registration,'');
  for(const id of ['filter-A','filter-missing'])assert.ok(repeat.find(r=>r.scenario_id===id).body.some(d=>d.id===blank.id));
  const matched=read('repeat/manifest.json').devices.find(d=>d.label==='contract').configuration;
  assert.ok(!repeat.find(r=>r.scenario_id==='filter-missing').body.some(d=>d.id===matched.id));
});
