import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ReactFlow, Background, Controls, MiniMap, Handle, Position, addEdge, applyNodeChanges, applyEdgeChanges } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './style.css';
import { xyNodes, xyEdges, toGraph, connectionError } from './graph.js';
import { nodeExecutionState } from './telemetry.js';

const example = {
  name: 'hello', queue_capacity: 2,
  nodes: [{id:'source',unit:'core.input'},{id:'upper',unit:'text.uppercase'},{id:'prefix',unit:'text.prefix',config:{prefix:'ModelBox Rust says: '}},{id:'result',unit:'core.output'}],
  edges:[{from:'source.out',to:'upper.in'},{from:'upper.out',to:'prefix.in'},{from:'prefix.out',to:'result.in'}]
};
function download(name,obj){const blob=new Blob([JSON.stringify(obj,null,2)+'\n'],{type:'application/json'});const url=URL.createObjectURL(blob);const a=document.createElement('a');a.href=url;a.download=name;a.click();URL.revokeObjectURL(url);}
function UnitNode({id,data}){
  const d=data.descriptor;
  return <div className={"unit"+(data.failure?" unit-failed":"")}><strong>{id}</strong><small>{data.unit}</small>{data.failure&&<div className="execution-state failed">Runtime 报告此节点执行失败</div>}{data.execution&&<div className={"execution-state "+data.execution.kind}>{data.execution.label}</div>}
    {(d?.inputs||[]).map(p=><div className="port in" key={p.name}><Handle type="target" position={Position.Left} id={p.name} style={{top:55+(d.inputs.indexOf(p))*24}}/>{p.name} · {p.data_type}</div>)}
    {(d?.outputs||[]).map(p=><div className="port out" key={p.name}><Handle type="source" position={Position.Right} id={p.name} style={{top:55+(d.inputs.length+p.name.length*0)*0+(d.outputs.indexOf(p))*24}}/>{p.name} · {p.data_type}</div>)}
  </div>;
}
function TracePanel({events,droppedEvents,onSelect}) {
  const end = Math.max(1, ...events.map(e => e.process_started_ns + e.process_duration_ns));
  const visible = events.slice(0, 100);
  return <div className="trace-panel" aria-label="逐帧运行轨迹">
    <h4>FlowUnit 逐帧处理时间线</h4>
    <p>已保留 {events.length} 条，采集丢弃 {droppedEvents} 条；当前显示前 {visible.length} 条。</p>
    <p>单调时钟相对偏移，仅包含算子处理，不含等待、队列或传输；时间条可重叠。</p>
    {visible.map((event,i)=><button className="trace-event" key={i} onClick={()=>onSelect(event.node_id)} title="定位对应 FlowUnit">
      <strong>{event.node_id} · 帧 {event.sequence}</strong>
      <span className="trace-track"><span className="trace-bar" style={{left:(event.process_started_ns/end*100)+'%',width:'max(2px, '+(event.process_duration_ns/end*100)+'%)'}}/></span>
      <small>{(event.process_started_ns/1e6).toFixed(3)} ms + {(event.process_duration_ns/1e6).toFixed(3)} ms · {event.ok?'完成':'失败'}</small>
    </button>)}
  </div>;
}

function EdgePanel({edges,deliveries,dropped,onSelect}) {
  return <div className="edge-panel" aria-label="逐边传输诊断">
    <h4>Edge 数据传输诊断</h4>
    <p>已识别 {edges.length} 条 Edge，已保留 {deliveries.length} 条逐帧交付事件，丢弃 {dropped} 条。</p>
    <p>发送尝试开始 → 接收完成，包含背压及调度耗时，不能视为精确的队列驻留时间。</p>
    {edges.slice(0,25).map((edge,i)=><button className="edge-entry" key={i}
      onClick={()=>onSelect(edge.to.slice(0,edge.to.lastIndexOf('.')))} title="选中下游节点">
      <strong>{edge.from} → {edge.to}</strong>
      <small>交付 {edge.delivered_frames} 帧；发送 {edge.send_operations} 次 / {(edge.send_wall_ns/1e6).toFixed(3)} ms；接收 {edge.receive_operations} 次 / {(edge.receive_wall_ns/1e6).toFixed(3)} ms；超时重试 {edge.send_timeouts} 次</small>
    </button>)}
    {edges.length>25&&<p>仅显示前 25 条 Edge。</p>}
    <h4>逐帧交付事件</h4>
    {deliveries.slice(0,30).map((event,i)=><div className="edge-event" key={i}>
      <strong>{event.from} → {event.to} · 帧 {event.sequence}</strong>
      <small>发送尝试起点 {(event.send_attempt_started_ns/1e6).toFixed(3)} ms；接收完成 {(event.receive_completed_ns/1e6).toFixed(3)} ms；区间 {(event.attempt_to_receive_ns/1e6).toFixed(3)} ms</small>
    </div>)}
    {deliveries.length>30&&<p>逐帧事件仅显示前 30 条。</p>}
  </div>;
}
const nodeTypes={flowunit:UnitNode};
function App(){
  const [catalog,setCatalog]=useState([]),[nodes,setNodes]=useState(xyNodes(example)),[edges,setEdges]=useState(xyEdges(example));
  const [name,setName]=useState(example.name),[capacity,setCapacity]=useState(example.queue_capacity),[selected,setSelected]=useState(null);
  const [input,setInput]=useState('hello modelbox'),[status,setStatus]=useState('未连接 Runtime'),[busy,setBusy]=useState(false);
  const [configDraft,setConfigDraft]=useState('null');
  const [configDirty,setConfigDirty]=useState(false);
  const [lastRun,setLastRun]=useState(null);
  const [profileSnapshot,setProfileSnapshot]=useState(null);
  const [failureSnapshot,setFailureSnapshot]=useState(null);
  useEffect(()=>{fetch('/api/catalog').then(r=>r.json()).then(d=>{if(!Array.isArray(d))throw Error(d.error||'invalid catalog');setCatalog(d);setStatus('Rust FlowUnit Registry 已连接');}).catch(e=>setStatus('连接失败：'+e.message));},[]);
  const descriptors=useMemo(()=>Object.fromEntries(catalog.map(x=>[x.name,x])),[catalog]);
  const flowNodes=useMemo(()=>nodes.map(n=>({...n,selected:n.id===selected,data:{...n.data,descriptor:descriptors[n.data.unit],execution:nodeExecutionState(n.id,n.data.unit,profileSnapshot?.profile,profileSnapshot?.graph===JSON.stringify(toGraph(nodes,edges,name,capacity))),failure:failureSnapshot?.nodeId===n.id && failureSnapshot?.graph===JSON.stringify(toGraph(nodes,edges,name,capacity))}})),[nodes,edges,name,capacity,descriptors,profileSnapshot,failureSnapshot,selected]);
  const flowEdges=useMemo(()=>{
    if(lastRun?.action!=='run-edges' || lastRun.graph!==JSON.stringify(toGraph(nodes,edges,name,capacity))) return edges;
    return edges.map(edge=>{
      const from=edge.source+'.'+edge.sourceHandle;
      const to=edge.target+'.'+edge.targetHandle;
      const stat=lastRun.edge_profiles?.find(item=>item.from===from && item.to===to);
      return stat?{...edge,label:stat.delivered_frames+' 帧',labelStyle:{fill:'#164863',fontSize:12,fontWeight:600}}:edge;
    });
  },[edges,nodes,name,capacity,lastRun]);
  const onConnect=useCallback(c=>{
    const problem=connectionError(c,nodes,edges,descriptors);
    if(problem){setStatus(problem);return;}
    setEdges(es=>addEdge({...c,id:'edge-'+crypto.randomUUID()},es));
  },[nodes,edges,descriptors]);
  const spec=()=>toGraph(nodes,edges,name,capacity);
  function guardSaved(){if(configDirty){setStatus('当前节点配置有未应用修改，请先点击“应用节点配置”');return false;}return true;}
  function addUnit(d){let i=1;while(nodes.some(n=>n.id==='node'+i))i++;setNodes(ns=>[...ns,{id:'node'+i,type:'flowunit',position:{x:100+ns.length*25,y:100+ns.length*30},data:{unit:d.name,config:d.required_string_config_keys.length?Object.fromEntries(d.required_string_config_keys.map(k=>[k,''])):null}}]);}
  async function execute(action){
    if(!guardSaved())return;
    setBusy(true);
    setLastRun(null);
    setProfileSnapshot(null);
    setFailureSnapshot(null);
    try {
      const response=await fetch('/api/'+action,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({graph:spec(),input})});
      const data=await response.json();
      setLastRun({action,ok:response.ok,diagnostics:data.diagnostics??null,result:data.result??null,node_process:data.node_process??null,node_transport:data.node_transport??null,edge_profiles:data.edges??null,deliveries:data.deliveries??null,dropped_deliveries:data.dropped_deliveries??0,events:data.events??null,dropped_events:data.dropped_events??0,graph:JSON.stringify(spec()),error:data.error??null,message:data.message??null});
      if(!response.ok && typeof data.node_id==='string') setFailureSnapshot({nodeId:data.node_id,graph:JSON.stringify(spec()),message:data.error||'Runtime execution failed'});
      if(response.ok && action==='run-profile' && data.node_process) setProfileSnapshot({graph:JSON.stringify(spec()),profile:data.node_process});
      setStatus(response.ok ? action==='validate' ? '校验成功' : '执行成功' : '执行/校验失败');
    } catch(e) {
      setLastRun({action,ok:false,diagnostics:null,result:null,node_process:null,error:e.message,message:null});
      setStatus('调用失败：'+e.message);
    } finally {setBusy(false);}
  }
  async function load(event){
    const file=event.target.files?.[0];if(!file)return;
    if(!guardSaved()){event.target.value='';return;}
    try{
      let json,description='已导入 '+file.name;
      if(/\.(dot|toml|conf)$/i.test(file.name)){
        if(file.size>128*1024)throw Error('旧版配置不能超过 128 KiB');
        const response=await fetch('/api/import-legacy',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({filename:file.name,legacy_text:await file.text()})});
        const migrated=await response.json();
        if(!response.ok)throw Error(migrated.error||'旧版迁移失败');
        json={graph:migrated.graph};
        description='已迁移兼容子集 '+file.name+'（'+migrated.report.node_count+' 节点 / '+migrated.report.edge_count+' 连线，Rust 校验通过）；旧版驱动未迁移';
      }else{
        json=JSON.parse(await file.text());
      }
      const graph=json.graph||json;
      if(!Array.isArray(graph.nodes)||!Array.isArray(graph.edges))throw Error('缺少 GraphSpec nodes/edges');
      setName(graph.name);setCapacity(graph.queue_capacity??16);setNodes(xyNodes(graph,json.positions));setEdges(xyEdges(graph));setSelected(null);setConfigDirty(false);setStatus(description);
    }catch(e){setStatus('导入失败：'+e.message);}event.target.value='';
  }
  const current=nodes.find(n=>n.id===selected);
  const activeFailure=failureSnapshot?.graph===JSON.stringify(spec()) && nodes.some(n=>n.id===failureSnapshot.nodeId) ? failureSnapshot : null;
  useEffect(()=>{setConfigDraft(JSON.stringify(current?.data.config??null,null,2));setConfigDirty(false);},[selected]);
  function applyConfig(){if(!current)return;try{const parsed=JSON.parse(configDraft);setNodes(ns=>ns.map(n=>n.id===current.id?{...n,data:{...n.data,config:parsed}}:n));setConfigDirty(false);setStatus('节点配置已应用；请运行 Rust 校验');}catch(e){setStatus('配置 JSON 无效：'+e.message);}}
  return <div className="app"><header><strong>ModelBox Studio</strong><span>v0.2 · 实际 Rust Graph 编排</span><input value={name} onChange={e=>setName(e.target.value)} aria-label="Graph name"/><label>队列 <input type="number" min="1" max="65536" value={capacity} onChange={e=>setCapacity(e.target.value)}/></label></header>
    <div className="body"><aside><h3>FlowUnits · {catalog.length}</h3>{catalog.map(d=><button key={d.name} onClick={()=>addUnit(d)} title="添加节点">{d.name} <span>＋</span></button>)}<p>节点与端口均来自 Rust Registry。未连接时不能添加节点。</p></aside>
      <main><ReactFlow nodes={flowNodes} edges={flowEdges} nodeTypes={nodeTypes} onNodesChange={c=>setNodes(n=>applyNodeChanges(c,n))} onEdgesChange={c=>setEdges(e=>applyEdgeChanges(c,e))} onConnect={onConnect} onNodeClick={(_,n)=>setSelected(n.id)} onPaneClick={()=>setSelected(null)} fitView><Background/><Controls/><MiniMap/></ReactFlow></main>
      <section><h3>属性与运行</h3>{current?<><div>节点：{current.id}</div><div>算子：{current.data.unit}</div><label>配置 JSON<textarea value={configDraft} onChange={e=>{setConfigDraft(e.target.value);setConfigDirty(true);}}/></label><button onClick={applyConfig}>应用节点配置</button><button onClick={()=>{setNodes(ns=>ns.filter(n=>n.id!==current.id));setEdges(es=>es.filter(e=>e.source!==current.id&&e.target!==current.id));setSelected(null);}}>删除节点</button></>:<p>单击节点以修改配置。选择连线后按 Delete 可删除。</p>}
      {activeFailure&&<div className="failure-inspector" aria-label="故障节点详情"><h4>Runtime 故障定位</h4><p>节点：<strong>{activeFailure.nodeId}</strong></p><pre>{activeFailure.message}</pre><button onClick={()=>{if(configDirty){setStatus('请先应用当前配置再定位故障节点');return;}setSelected(activeFailure.nodeId);}}>选择故障节点</button><p>此标记来自 Rust Runtime 的结构化节点 ID。编辑 Graph 后自动失效。</p></div>}
      <h4>Graph 文件</h4><button onClick={()=>{if(guardSaved())download(name+'.json',spec());}}>导出 GraphSpec</button><button onClick={()=>{if(guardSaved())download(name+'.studio.json',{graph:spec(),positions:Object.fromEntries(nodes.map(n=>[n.id,n.position]))});}}>保存画布</button><label className="upload">导入 JSON<input type="file" accept=".json,.dot,.toml,.conf,application/json,text/plain" onChange={load}/></label>
      <h4>Rust Runtime</h4><input value={input} onChange={e=>setInput(e.target.value)} aria-label="运行输入文本"/><button disabled={busy} onClick={()=>execute('validate')}>校验 Graph</button><button disabled={busy} onClick={()=>execute('run')}>执行 Graph</button><button disabled={busy} onClick={()=>execute('run-profile')}>分析节点耗时</button><button disabled={busy} onClick={()=>execute('run-trace')}>查看逐帧轨迹</button><button disabled={busy} onClick={()=>execute('run-transport')}>查看通道API计时</button><button disabled={busy} onClick={()=>execute('run-edges')}>分析逐边交付</button><pre role="status">{status}</pre>{lastRun&&<div className="run-report" aria-label="Runtime 诊断"><h4>{lastRun.ok?'完成':'错误'} · {lastRun.action}</h4>{lastRun.diagnostics&&<p>CLI 端到端耗时：{lastRun.diagnostics.elapsed_ms} ms（含进程启动/可能的编译，非推理耗时）</p>}{lastRun.error&&<pre role="alert">{lastRun.error}</pre>}{lastRun.message&&<pre>{lastRun.message}</pre>}{lastRun.result&&<><p>输入数量：{lastRun.result.input_count}</p><pre>{JSON.stringify(lastRun.result.outputs,null,2)}</pre></>}{lastRun.node_process&&<div aria-label="节点处理耗时"><h4>FlowUnit 处理统计（不含调度及队列）</h4>{Object.entries(lastRun.node_process).map(([id,p])=><p key={id}><strong>{id}</strong> · {p.calls} 次 · {(p.process_ns/1e6).toFixed(3)} ms 累计</p>)}<p>各节点可能并行，耗时不能相加视为整图延迟。</p></div>}{lastRun.node_transport && lastRun.graph===JSON.stringify(spec()) && <div aria-label="通道API计时"><h4>有界通道 API 耗时</h4><p>接收耗时包括上游处理与调度等待；发送耗时包括接口开销与背压。不是纯队列等待或网络延迟。</p>{Object.entries(lastRun.node_transport).map(([id,p])=><div className="channel-profile" key={id}><strong>{id}</strong><p>接收 {p.receive_operations} 次 · {(p.receive_wall_ns/1e6).toFixed(3)} ms</p><p>发送 {p.send_operations} 次 · {(p.send_wall_ns/1e6).toFixed(3)} ms · 超时重试 {p.send_timeouts} 次</p></div>)}</div>}{lastRun.edge_profiles && lastRun.deliveries && lastRun.graph===JSON.stringify(spec()) && <EdgePanel edges={lastRun.edge_profiles} deliveries={lastRun.deliveries} dropped={lastRun.dropped_deliveries} onSelect={setSelected}/>} {lastRun.events && lastRun.graph===JSON.stringify(spec()) && <TracePanel events={lastRun.events} droppedEvents={lastRun.dropped_events} onSelect={setSelected}/>}</div>}</section></div></div>;
}
createRoot(document.getElementById('root')).render(<App/>);
