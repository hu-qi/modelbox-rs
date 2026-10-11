import test from 'node:test';
import assert from 'node:assert/strict';
import { xyNodes, xyEdges, toGraph, connectionError } from '../src/graph.js';

const original = {
  name: 'hello', queue_capacity: 2,
  nodes: [
    { id: 'source', unit: 'core.input' },
    { id: 'prefix', unit: 'text.prefix', config: { prefix: 'ModelBox: ' } },
    { id: 'result', unit: 'core.output' }
  ],
  edges: [
    { from: 'source.out', to: 'prefix.in' },
    { from: 'prefix.out', to: 'result.in' }
  ]
};
const descriptors = {
  'core.input': { inputs: [], outputs: [{ name: 'out', data_type: 'text' }] },
  'text.prefix': { inputs: [{ name: 'in', data_type: 'text' }], outputs: [{ name: 'out', data_type: 'text' }] },
  'core.output': { inputs: [{ name: 'in', data_type: 'text' }], outputs: [{ name: 'out', data_type: 'text' }] },
  'tensor.input': { inputs: [], outputs: [{ name: 'out', data_type: 'tensor_f32' }] }
};

test('executable GraphSpec round trip does not leak canvas positions', () => {
  const nodes = xyNodes(original, { source: { x: 50, y: 100 } });
  assert.deepEqual(nodes[0].position, { x: 50, y: 100 });
  assert.deepEqual(toGraph(nodes, xyEdges(original), original.name, 2), original);
});
test('multi-dot node identifiers preserve source/target endpoint boundary', () => {
  assert.deepEqual(xyEdges({ edges: [{ from: 'source.out', to: 'result.in' }] })[0].targetHandle, 'in');
});
test('unknown ports, mismatched data types, occupied inputs, and self-loops are rejected', () => {
  const nodes = xyNodes(original);
  const valid = { source: 'source', sourceHandle: 'out', target: 'prefix', targetHandle: 'in' };
  assert.equal(connectionError(valid, nodes, [], descriptors), null);
  assert.match(connectionError(valid, nodes, xyEdges(original), descriptors), /已有上游/);
  assert.match(connectionError({ ...valid, sourceHandle: 'bad' }, nodes, [], descriptors), /不存在/);
  const tensor = { id: 'tensor', data: { unit: 'tensor.input' } };
  assert.match(connectionError({ ...valid, source: 'tensor' }, [...nodes, tensor], [], descriptors), /类型不匹配/);
  assert.match(connectionError({ ...valid, source: 'prefix' }, nodes, [], descriptors), /自身/);
});
