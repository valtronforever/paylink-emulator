import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

test('absent reference recordings are optional unless explicitly required', () => {
  const cwd = mkdtempSync(join(tmpdir(), 'paylink-reference-policy-'));
  try {
    for (const required of [false, true]) {
      const run = spawnSync(process.execPath, [fileURLToPath(new URL('./differential.mjs', import.meta.url)), ...(required ? ['--require-reference'] : [])], {cwd, encoding:'utf8'});
      assert.equal(run.status, required ? 2 : 0, run.stderr);
      const report = JSON.parse(readFileSync(join(cwd,'test-results/differential.json')));
      assert.equal(report.status, required ? 'blocked' : 'not_run');
      assert.equal(report.required, required);
      assert.equal(report.verified, 0);
    }
  } finally { rmSync(cwd, {recursive:true,force:true}); }
});
