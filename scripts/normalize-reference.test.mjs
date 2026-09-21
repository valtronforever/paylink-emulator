import {test} from 'node:test';
import assert from 'node:assert/strict';
import {normalizeReference as normalized} from './normalize-reference.mjs';
test('normalize explicit identifiers without hiding type/nullability/contract differences', () => {
  const fields = ['id', 'result.invoice_num', 'result.receipt_no'];
  const reference = {id:'uuid-reference', result:{invoice_num:12,receipt_no:'12',amount:100}};
  const actual = {id:'g1-op1', result:{invoice_num:1,receipt_no:'1',amount:100}};
  assert.deepEqual(normalized(reference, fields), normalized(actual, fields));
  assert.equal(reference.id, 'uuid-reference');
  for (const body of [
    {result:{rrn:null}}, {result:{}}, {result:{rrn:123}}, {result:{rrn:'123'}},
  ]) {
    const others = [{result:{rrn:null}}, {result:{}}, {result:{rrn:123}}, {result:{rrn:'123'}}];
    for (const other of others) {
      assert.equal(JSON.stringify(normalized(body,['result.rrn'])) === JSON.stringify(normalized(other,['result.rrn'])), JSON.stringify(body) === JSON.stringify(other));
    }
  }
  assert.throws(() => normalized({result:{rrn:{}}}, ['result.rrn']), /scalar/);
  assert.throws(() => normalized(actual, ['result.amount']), /allowed/);
  assert.notDeepEqual(normalized({...actual,success:true}, fields),normalized({...reference,success:false},fields));
});
