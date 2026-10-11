import test from 'node:test';
import assert from 'node:assert/strict';
import { nodeExecutionState } from '../src/telemetry.js';

test('no profile or a changed graph produces no visual execution claim', () => {
  assert.equal(nodeExecutionState('upper','text.uppercase',null,true),null);
  assert.equal(nodeExecutionState('upper','text.uppercase',{upper:{calls:1,process_ns:10}},false),null);
});
test('Rust node sample is used without inferring graph latency', () => {
  assert.deepEqual(nodeExecutionState('upper','text.uppercase',{upper:{calls:2,process_ns:1500000}},true),{kind:'processed',label:'已处理 2 次 · 1.500 ms'});
});
test('source is not considered failed, missing samples are not errors', () => {
  assert.equal(nodeExecutionState('source','core.input',{},true).kind,'source');
  assert.equal(nodeExecutionState('upper','text.uppercase',{},true).kind,'unobserved');
});
