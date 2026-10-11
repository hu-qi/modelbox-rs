// Pure Studio graph conversion and link validation, shared with Node CI tests.
export function xyNodes(spec, positions = {}) {
  return spec.nodes.map((n, i) => ({
    id: n.id,
    type: 'flowunit',
    position: positions[n.id] || { x: 80 + (i % 3) * 260, y: 80 + Math.floor(i / 3) * 180 },
    data: { unit: n.unit, config: n.config ?? null }
  }));
}
export function xyEdges(spec) {
  return spec.edges.map((edge, i) => {
    const split = value => {
      const index = value.lastIndexOf('.');
      if (index <= 0 || index === value.length - 1) throw Error('Invalid edge endpoint: ' + value);
      return [value.slice(0, index), value.slice(index + 1)];
    };
    const [source, sourceHandle] = split(edge.from);
    const [target, targetHandle] = split(edge.to);
    return { id: 'edge-' + i + '-' + edge.from + '-' + edge.to, source, sourceHandle, target, targetHandle };
  });
}
export function toGraph(nodes, edges, name, capacity) {
  return {
    name, queue_capacity: Number(capacity),
    nodes: nodes.map(n => ({ id: n.id, unit: n.data.unit, ...(n.data.config == null ? {} : { config: n.data.config }) })),
    edges: edges.map(e => ({ from: e.source + '.' + e.sourceHandle, to: e.target + '.' + e.targetHandle }))
  };
}
export function connectionError(connection, nodes, edges, descriptors) {
  const source = nodes.find(n => n.id === connection.source);
  const target = nodes.find(n => n.id === connection.target);
  const out = descriptors[source?.data.unit]?.outputs.find(p => p.name === connection.sourceHandle);
  const input = descriptors[target?.data.unit]?.inputs.find(p => p.name === connection.targetHandle);
  if (!out || !input) return '端口不存在';
  if (out.data_type !== input.data_type) return '端口类型不匹配';
  if (edges.some(e => e.target === connection.target && e.targetHandle === connection.targetHandle))
    return '目标输入端口已有上游';
  if (connection.source === connection.target) return '节点不能连接自身';
  return null;
}
