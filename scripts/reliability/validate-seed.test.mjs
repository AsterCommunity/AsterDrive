import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const validator = fileURLToPath(new URL('./validate-seed.mjs', import.meta.url));

test('accepts the decimal u64 syntax supported by Rust', () => {
  for (const seed of ['0', '522', '+522', '000522', '18446744073709551615', '+00018446744073709551615']) {
    const result = spawnSync(process.execPath, [validator, seed], { encoding: 'utf8' });
    assert.equal(result.status, 0, `${seed}: ${result.stderr}`);
  }
});

test('rejects invalid syntax and u64 overflow before scenario preparation', () => {
  for (const seed of ['', '+', '-1', ' 522', '522 ', '522\n', '522\r', '1.0', '1e3', '0xff', '１２', '18446744073709551616']) {
    const result = spawnSync(process.execPath, [validator, seed], { encoding: 'utf8' });
    assert.equal(result.status, 1, seed);
    assert.match(result.stderr, /SEED must be an unsigned 64-bit integer/);
  }
});
