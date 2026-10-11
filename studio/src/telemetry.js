// Display states derive ONLY from a successful, opt-in Rust profile for the current graph.
export function nodeExecutionState(nodeId, unit, profile, isCurrent) {
  if (!isCurrent || !profile) return null;
  if (Object.prototype.hasOwnProperty.call(profile, nodeId)) {
    const sample = profile[nodeId];
    return { label: `已处理 ${sample.calls} 次 · ${(sample.process_ns / 1e6).toFixed(3)} ms`, kind: 'processed' };
  }
  if (unit === 'core.input' || unit === 'tensor.input') {
    return { label: '源节点 · 不统计 process_owned', kind: 'source' };
  }
  return { label: '未采集到处理样本', kind: 'unobserved' };
}
