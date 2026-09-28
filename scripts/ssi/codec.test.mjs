import {test} from 'node:test';
import assert from 'node:assert/strict';
import {Decoder, encode, lrc} from './codec.mjs';
// Independently counted UTF-8 bytes / XOR, following spec §§1.3, 6.1.
// The PDF supplies an algorithm, not a complete golden packet.
const golden = Buffer.from('02660100177b226d6574686f64223a2250696e67446576696365227d2b', 'hex');
test('independent PingDevice packet', () => {
  assert.equal(lrc(Buffer.from('{"method":"PingDevice"}')), 0x2b);
  assert.deepEqual(encode({method: 'PingDevice'}), golden);
});
test('every split and concatenated packets', () => {
  for (let split = 0; split <= golden.length; split++) {
    const d = new Decoder();
    const frames = [...d.push(golden.subarray(0, split)), ...d.push(Buffer.concat([golden.subarray(split), golden]))];
    assert.deepEqual(frames.map(f => f.json), [{method: 'PingDevice'}, {method: 'PingDevice'}]);
    assert.equal(d.pending.length, 0);
  }
});
test('UTF-8 byte length, checksum, JSON, prefix and uint16 limit', () => {
  const utf = encode({text: 'ї'}); assert.equal(utf.readUInt16BE(3), 13);
  assert.deepEqual(new Decoder().push(utf)[0].json, {text: 'ї'});
  const corrupt = Buffer.from(golden); corrupt[corrupt.length - 1] ^= 1;
  assert.equal(new Decoder().push(corrupt)[0].error, 'E02');
  const badJson = Buffer.from('02660100012121', 'hex');
  assert.equal(new Decoder().push(badJson)[0].error, 'E03');
  const wrong = Buffer.from(golden); wrong[2] = 2;
  assert.equal(new Decoder().push(wrong)[0].fatal, true);
  assert.throws(() => encode({x: 'x'.repeat(65536)}), RangeError);
});
