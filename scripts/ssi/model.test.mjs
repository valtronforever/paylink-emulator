import {test} from 'node:test';
import assert from 'node:assert/strict';
import {Terminal, scenario} from './model.mjs';
const purchase = {method: 'Purchase', params: {transAmount: '100', transCurrency: '980', merchantId: 'TEST-MERCHANT'}};
test('acceptance, completion and last-result independent of polling; snapshot survives arm', () => {
  let now = 0; const events = [], t = new Terminal((...e) => events.push(e), () => now);
  t.arm({id: 'first', phases: [{status: 'S02', ms: 10}]});
  assert.equal(t.handle(purchase).error, false);
  t.arm({id: 'next', error_code: 'E10'});
  assert.equal(t.handle(purchase).errorCode, 'E06');
  assert.equal(t.handle({method: 'GetStatus'}).status, 'S02');
  now = 10; t.tick();
  assert.equal(t.handle({method: 'GetLastResult'}).error, false);
  assert.equal(events.filter(e => e[0] === 'completed').length, 1);
  t.handle(purchase); now = 1000;
  assert.equal(t.handle({method: 'GetLastResult'}).errorCode, 'E10');
});
test('interrupt phase restrictions, rejected requests preserve last result, reset semantics', () => {
  let now = 0; const t = new Terminal(() => {}, () => now);
  t.arm({id: 'cancel', phases: [{status: 'S03', ms: 100}]}); t.handle(purchase);
  assert.equal(t.handle({method: 'Interrupt'}).error, false);
  assert.equal(t.handle({method: 'GetLastResult'}).errorCode, 'E12');
  assert.equal(t.handle({method: 'Purchase'}).errorCode, 'E04');
  assert.equal(t.handle({method: 'GetLastResult'}).errorCode, 'E12');
  t.reset({preserve_result: true}); assert.equal(t.last.code, 'E12');
  t.reset(); assert.equal(t.last, null);
  assert.equal(t.handle({method: 'MadeUp'}).errorCode, 'E05');
  t.arm({id: 'bank', phases: [{status: 'S04', ms: 100}]}); t.handle(purchase);
  assert.equal(t.handle({method: 'Interrupt'}).errorCode, 'E08');
});
test('scenario validation rejects silent typos and unbounded waits', () => {
  assert.throws(() => scenario({delay: 2}));
  assert.throws(() => scenario({phases: [{status: 'S04', ms: -1}]}));
  assert.throws(() => scenario({faults: {Unknown: 'silence'}}));
  assert.throws(() => scenario({error_code: 'E23'}));
});
