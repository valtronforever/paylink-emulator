import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve,sep} from 'node:path';
import {createHash} from 'node:crypto';
test('reviewed experiment files retain their recorded hashes and explicit coverage gaps', () => {
  const root=resolve('profiles/desktop-paylink-2.1.20-win-x86/reference/experiments');
  const index=JSON.parse(readFileSync(resolve(root,'index.json')));
  assert.ok(index.files.length>0);
  for(const entry of index.files){
    const file=resolve(root,entry.path);
    assert.ok(file.startsWith(root+sep),'Evidence must stay in the package');
    assert.equal(createHash('sha256').update(readFileSync(file)).digest('hex'),entry.sha256,entry.path);
  }
  const inventory=JSON.parse(readFileSync(resolve(root,'../../ssi/inventory.json')));
  assert.equal(inventory.errors.length,23);assert.equal(inventory.states.length,9);assert.equal(inventory.financial_results.length,31);
  for(const row of [...inventory.errors,...inventory.states,...inventory.financial_results,...inventory.transport]){
    assert.ok(row.reason?.length>0,`${row.code}: explain scope or missing support`);
    if(row.raw_ssi)readFileSync(resolve(root,'../../',row.raw_ssi));
  }
});
