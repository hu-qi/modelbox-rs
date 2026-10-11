import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { ReactFlow, Background, Controls, MiniMap, Handle, Position, addEdge, applyNodeChanges, applyEdgeChanges } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './style.css';
import { xyNodes, xyEdges, toGraph, connectionError } from './graph.js';

const example = {
  name: 'hello', queue_capacity: 2,
  nodes: [{id:'source',unit:'core.input'},{id:'upper',unit:'text.uppercase'},{id:'prefix',unit:'text.prefix',config:{prefix:'ModelBox Rust says: '}},{id:'result',unit:'core.output'}],
  edges:[{from:'source.out',to:'upper.in'},{from:'upper.out',to:'prefix.in'},{from:'prefix.out',to:'result.in'}]
};
function download(name,obj){const blob=new Blob([JSON.stringify(obj,null,2)+'\n'],{type:'application/json'});const url=URL.createObjectURL(blob);const a=document.createElement('a');a.href=url;a.download=name;a.click();URL.revokeObjectURL(url);}
function UnitNode({id,data}){
  const d=data.descriptor;
  return <div className="unit"><strong>{id}</strong><small>{data.unit}</small>
    {(d?.inputs||[]).map(p=><div className="port in" key={p.name}><Handle type="target" position={Position.Left} id={p.name} style={{top:55+(d.inputs.indexOf(p))*24}}/>{p.name} · {p.data_type}</div>)}
    {(d?.outputs||[]).map(p=><div className="port out" key={p.name}><Handle type="source" position={Position.Right} id={p.name} style={{top:55+(d.inputs.length+p.name.length*0)*0+(d.outputs.indexOf(p))*24}}/>{p.name} · {p.data_type}</div>)}
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
  useEffect(()=>{fetch('/api/catalog').then(r=>r.json()).then(d=>{if(!Array.isArray(d))throw Error(d.error||'invalid catalog');setCatalog(d);setStatus('Rust FlowUnit Registry 已连接');}).catch(e=>setStatus('连接失败：'+e.message));},[]);
  const descriptors=useMemo(()=>Object.fromEntries(catalog.map(x=>[x.name,x])),[catalog]);
  const flowNodes=useMemo(()=>nodes.map(n=>({...n,data:{...n.data,descriptor:descriptors[n.data.unit]}})),[nodes,descriptors]);
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
    try {
      const response=await fetch('/api/'+action,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({graph:spec(),input})});
      const data=await response.json();
      setLastRun({action,ok:response.ok,diagnostics:data.diagnostics??null,result:data.result??null,error:data.error??null,message:data.message??null});
      setStatus(response.ok ? action==='run' ? '执行成功' : '校验成功' : '执行/校验失败');
    } catch(e) {
      setLastRun({action,ok:false,diagnostics:null,result:null,error:e.message,message:null});
      setStatus('调用失败：'+e.message);
    } finally {setBusy(false);}
  }
  async function load(event){
    const file=event.target.files?.[0];if(!file)return;
    if(!guardSaved()){event.target.value='';return;}
    try{const json=JSON.parse(await file.text());const graph=json.graph||json;if(!Array.isArray(graph.nodes)||!Array.isArray(graph.edges))throw Error('缺少 GraphSpec nodes/edges');
      setName(graph.name);setCapacity(graph.queue_capacity??16);setNodes(xyNodes(graph,json.positions));setEdges(xyEdges(graph));setSelected(null);setConfigDirty(false);setStatus('已导入 '+file.name);
    }catch(e){setStatus('导入失败：'+e.message);}event.target.value='';
  }
  const current=nodes.find(n=>n.id===selected);
  useEffect(()=>{setConfigDraft(JSON.stringify(current?.data.config??null,null,2));setConfigDirty(false);},[selected]);
  function applyConfig(){if(!current)return;try{const parsed=JSON.parse(configDraft);setNodes(ns=>ns.map(n=>n.id===current.id?{...n,data:{...n.data,config:parsed}}:n));setConfigDirty(false);setStatus('节点配置已应用；请运行 Rust 校验');}catch(e){setStatus('配置 JSON 无效：'+e.message);}}
  return <div className="app"><header><strong>ModelBox Studio</strong><span>v0.2 · 实际 Rust Graph 编排</span><input value={name} onChange={e=>setName(e.target.value)} aria-label="Graph name"/><label>队列 <input type="number" min="1" max="65536" value={capacity} onChange={e=>setCapacity(e.target.value)}/></label></header>
    <div className="body"><aside><h3>FlowUnits · {catalog.length}</h3>{catalog.map(d=><button key={d.name} onClick={()=>addUnit(d)} title="添加节点">{d.name} <span>＋</span></button>)}<p>节点与端口均来自 Rust Registry。未连接时不能添加节点。</p></aside>
      <main><ReactFlow nodes={flowNodes} edges={edges} nodeTypes={nodeTypes} onNodesChange={c=>setNodes(n=>applyNodeChanges(c,n))} onEdgesChange={c=>setEdges(e=>applyEdgeChanges(c,e))} onConnect={onConnect} onNodeClick={(_,n)=>setSelected(n.id)} onPaneClick={()=>setSelected(null)} fitView><Background/><Controls/><MiniMap/></ReactFlow></main>
      <section><h3>属性与运行</h3>{current?<><div>节点：{current.id}</div><div>算子：{current.data.unit}</div><label>配置 JSON<textarea value={configDraft} onChange={e=>{setConfigDraft(e.target.value);setConfigDirty(true);}}/></label><button onClick={applyConfig}>应用节点配置</button><button onClick={()=>{setNodes(ns=>ns.filter(n=>n.id!==current.id));setEdges(es=>es.filter(e=>e.source!==current.id&&e.target!==current.id));setSelected(null);}}>删除节点</button></>:<p>单击节点以修改配置。选择连线后按 Delete 可删除。</p>}
      <h4>Graph 文件</h4><button onClick={()=>{if(guardSaved())download(name+'.json',spec());}}>导出 GraphSpec</button><button onClick={()=>{if(guardSaved())download(name+'.studio.json',{graph:spec(),positions:Object.fromEntries(nodes.map(n=>[n.id,n.position]))});}}>保存画布</button><label className="upload">导入 JSON<input type="file" accept=".json,application/json" onChange={load}/></label>
      <h4>Rust Runtime</h4><input value={input} onChange={e=>setInput(e.target.value)} aria-label="运行输入文本"/><button disabled={busy} onClick={()=>execute('validate')}>校验 Graph</button><button disabled={busy} onClick={()=>execute('run')}>执行 Graph</button><pre role="status">{status}</pre>{lastRun&&<div className="run-report" aria-label="Runtime 诊断"><h4>{lastRun.ok?'完成':'错误'} · {lastRun.action}</h4>{lastRun.diagnostics&&<p>CLI 端到端耗时：{lastRun.diagnostics.elapsed_ms} ms（含进程启动/可能的编译，非推理耗时）</p>}{lastRun.error&&<pre role="alert">{lastRun.error}</pre>}{lastRun.message&&<pre>{lastRun.message}</pre>}{lastRun.result&&<><p>输入数量：{lastRun.result.input_count}</p><pre>{JSON.stringify(lastRun.result.outputs,null,2)}</pre></>}</div>}</section></div></div>;
}
createRoot(document.getElementById('root')).render(<App/>);
