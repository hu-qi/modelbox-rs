use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, SendTimeoutError, Sender};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{ExecutionPlan, FlowError, FlowRegistry, FlowUnit, GraphSpec, Result, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunResult {
    /// Unconnected outputs are exposed as node_id.port_name.
    /// Every vector preserves source sequence order.
    pub outputs: BTreeMap<String, Vec<Value>>,
    pub input_count: usize,
}

#[derive(Default)]
pub struct Runtime {
    registry: FlowRegistry,
}

#[derive(Clone)]
struct Frame {
    sequence: usize,
    value: Value,
    attempt: Option<EdgeAttempt>,
}

#[derive(Clone)]
struct EdgeAttempt {
    index: usize,
    started_at: Instant,
}

#[derive(Clone)]
struct EdgeSender {
    channel: Sender<Frame>,
    index: usize,
}

#[derive(Clone)]
struct EdgeReceiver {
    channel: Receiver<Frame>,
    index: usize,
}

type Captures = Arc<Mutex<BTreeMap<String, Vec<Frame>>>>;
type FirstError = Arc<Mutex<Option<FlowError>>>;
type ProfileStore = Arc<Mutex<BTreeMap<String, NodeProfile>>>;
type TransportStore = Arc<Mutex<BTreeMap<String, NodeTransportProfile>>>;
const MAX_TRACE_EVENTS: usize = 4096;
const MAX_EDGE_DELIVERIES: usize = 4096;

/// Aggregate time inside FlowUnit::process_owned only. Overlapping worker times
/// are not additive and do not represent graph end-to-end latency.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeProfile {
    pub calls: usize,
    pub process_ns: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfiledRun {
    pub result: RunResult,
    pub node_process: BTreeMap<String, NodeProfile>,
}

/// Duration spent inside bounded-channel send/receive APIs. These are wall
/// times, not isolated contention, queue residence, or network transport.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NodeTransportProfile {
    /// Includes successful receives and final disconnect/cancellation probes.
    pub receive_operations: usize,
    pub receive_wall_ns: u64,
    /// One per delivered/cancelled downstream frame dispatch attempt.
    pub send_operations: usize,
    pub send_wall_ns: u64,
    pub send_timeouts: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransportProfiledRun {
    pub result: RunResult,
    pub node_transport: BTreeMap<String, NodeTransportProfile>,
}


/// API-level per-edge measurements; neither duration is pure queue residence.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EdgeChannelProfile {
    pub from: String,
    pub to: String,
    pub send_operations: usize,
    pub send_wall_ns: u64,
    pub send_timeouts: usize,
    /// Includes the final end-of-stream receive probe.
    pub receive_operations: usize,
    pub receive_wall_ns: u64,
    pub delivered_frames: usize,
}

/// Interval measured from the start of a send attempt to completed receive.
/// Includes bounded-channel backpressure and scheduling, NOT queue-only dwell.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeDeliveryEvent {
    pub from: String,
    pub to: String,
    pub sequence: usize,
    pub send_attempt_started_ns: u64,
    pub receive_completed_ns: u64,
    pub attempt_to_receive_ns: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeTracedRun {
    pub result: RunResult,
    pub edges: Vec<EdgeChannelProfile>,
    pub deliveries: Vec<EdgeDeliveryEvent>,
    pub dropped_deliveries: usize,
}

struct EdgeTraceBuffer {
    edges: Vec<EdgeChannelProfile>,
    deliveries: Vec<EdgeDeliveryEvent>,
    dropped_deliveries: usize,
}

struct EdgeCollector {
    origin: Instant,
    buffer: Mutex<EdgeTraceBuffer>,
}

impl EdgeCollector {
    fn record_send(&self, index: usize, elapsed_ns: u64, timeouts: usize) {
        let mut buffer = self.buffer.lock().expect("edge mutex poisoned");
        let edge = &mut buffer.edges[index];
        edge.send_operations += 1;
        edge.send_wall_ns = edge.send_wall_ns.saturating_add(elapsed_ns);
        edge.send_timeouts += timeouts;
    }

    fn record_receive(&self, index: usize, elapsed_ns: u64, frame: Option<&Frame>, completed_at: Instant) {
        let mut buffer = self.buffer.lock().expect("edge mutex poisoned");
        {
            let edge = &mut buffer.edges[index];
            edge.receive_operations += 1;
            edge.receive_wall_ns = edge.receive_wall_ns.saturating_add(elapsed_ns);
            if frame.is_some() {
                edge.delivered_frames += 1;
            }
        }
        if let Some(stamp) = frame.and_then(|frame| frame.attempt.as_ref()) {
            debug_assert_eq!(stamp.index, index);
            if buffer.deliveries.len() < MAX_EDGE_DELIVERIES {
                let frame = frame.expect("stamp requires a frame");
                let (from, to) = {
                    let edge = &buffer.edges[index];
                    (edge.from.clone(), edge.to.clone())
                };
                buffer.deliveries.push(EdgeDeliveryEvent {
                    from,
                    to,
                    sequence: frame.sequence,
                    send_attempt_started_ns: stamp
                        .started_at
                        .duration_since(self.origin)
                        .as_nanos()
                        .min(u128::from(u64::MAX)) as u64,
                    receive_completed_ns: completed_at
                        .duration_since(self.origin)
                        .as_nanos()
                        .min(u128::from(u64::MAX)) as u64,
                    attempt_to_receive_ns: completed_at
                        .duration_since(stamp.started_at)
                        .as_nanos()
                        .min(u128::from(u64::MAX)) as u64,
                });
            } else {
                buffer.dropped_deliveries += 1;
            }
        }
    }
}

type EdgeStore = Arc<EdgeCollector>;

/// A bounded per-frame FlowUnit process interval (monotonic clock, not wall time).
/// Events contain no input/output payloads or tensor contents.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub node_id: String,
    pub sequence: usize,
    pub process_started_ns: u64,
    pub process_duration_ns: u64,
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TracedRun {
    pub result: RunResult,
    pub events: Vec<TraceEvent>,
    pub dropped_events: usize,
}

#[derive(Default)]
struct TraceBuffer {
    events: Vec<TraceEvent>,
    dropped_events: usize,
}

struct TraceCollector {
    origin: Instant,
    buffer: Mutex<TraceBuffer>,
}

impl TraceCollector {
    fn record(&self, event: TraceEvent) {
        let mut buffer = self.buffer.lock().expect("trace mutex poisoned");
        if buffer.events.len() >= MAX_TRACE_EVENTS {
            buffer.dropped_events += 1;
        } else {
            buffer.events.push(event);
        }
    }
}

type TraceStore = Arc<TraceCollector>;

fn remember_error(errors: &FirstError, cancelled: &AtomicBool, error: FlowError) {
    let mut guard = errors.lock().expect("error mutex poisoned");
    if guard.is_none() {
        *guard = Some(error);
    }
    cancelled.store(true, Ordering::Release);
}

fn receive(
    receiver: &EdgeReceiver,
    cancelled: &AtomicBool,
    node_id: &str,
    transport: Option<&TransportStore>,
    edge_store: Option<&EdgeStore>,
) -> Option<Frame> {
    let started = (transport.is_some() || edge_store.is_some()).then(Instant::now);
    let result = loop {
        if cancelled.load(Ordering::Acquire) {
            break None;
        }
        match receiver.channel.recv_timeout(Duration::from_millis(20)) {
            Ok(frame) => break Some(frame),
            Err(RecvTimeoutError::Disconnected) => break None,
            Err(RecvTimeoutError::Timeout) => {}
        }
    };
    if let Some(started) = started {
        let completed_at = Instant::now();
        let elapsed = completed_at
            .duration_since(started)
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        if let Some(store) = transport {
            let mut guard = store.lock().expect("transport mutex poisoned");
            let sample = guard.entry(node_id.to_owned()).or_default();
            sample.receive_operations += 1;
            sample.receive_wall_ns = sample.receive_wall_ns.saturating_add(elapsed);
        }
        if let Some(store) = edge_store {
            store.record_receive(receiver.index, elapsed, result.as_ref(), completed_at);
        }
    }
    result
}

/// Bounded transport keeps ownership of a timed-out frame for retries.
/// Cancellation and unexpected downstream disconnect retain their old semantics.
fn send_frame(
    node_id: &str,
    port_name: &str,
    sender: &EdgeSender,
    mut pending: Frame,
    cancelled: &AtomicBool,
    transport: Option<&TransportStore>,
    edge_store: Option<&EdgeStore>,
) -> Result<()> {
    let started = (transport.is_some() || edge_store.is_some()).then(Instant::now);
    if let (Some(_), Some(started)) = (edge_store, started) {
        pending.attempt = Some(EdgeAttempt {
            index: sender.index,
            started_at: started,
        });
    }
    let mut timeouts = 0;
    let result = loop {
        if cancelled.load(Ordering::Acquire) {
            break Ok(());
        }
        match sender.channel.send_timeout(pending, Duration::from_millis(20)) {
            Ok(()) => break Ok(()),
            Err(SendTimeoutError::Timeout(frame)) => {
                pending = frame;
                timeouts += 1;
            }
            Err(SendTimeoutError::Disconnected(_)) => {
                if cancelled.load(Ordering::Acquire) {
                    break Ok(());
                }
                break Err(FlowError::Execution(format!(
                    "downstream of {node_id}.{port_name} disconnected unexpectedly"
                )));
            }
        }
    };
    if let Some(started) = started {
        let elapsed = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        if let Some(store) = transport {
            let mut guard = store.lock().expect("transport mutex poisoned");
            let sample = guard.entry(node_id.to_owned()).or_default();
            sample.send_operations += 1;
            sample.send_wall_ns = sample.send_wall_ns.saturating_add(elapsed);
            sample.send_timeouts += timeouts;
        }
        if let Some(store) = edge_store {
            store.record_send(sender.index, elapsed, timeouts);
        }
    }
    result
}

struct DispatchContext<'a> {
    captures: &'a Captures,
    cancelled: &'a AtomicBool,
    transport: Option<&'a TransportStore>,
    edge_store: Option<&'a EdgeStore>,
}

fn dispatch(
    node_id: &str,
    unit: &dyn FlowUnit,
    sequence: usize,
    values: Vec<Value>,
    senders: &[Vec<EdgeSender>],
    context: &DispatchContext<'_>,
) -> Result<()> {
    if values.len() != unit.outputs().len() {
        return Err(FlowError::Execution(format!(
            "node {node_id} returned {} outputs, expected {}",
            values.len(),
            unit.outputs().len()
        )));
    }
    for (port_index, value) in values.into_iter().enumerate() {
        let port = unit.outputs()[port_index];
        if value.data_type() != port.data_type {
            return Err(FlowError::Execution(format!(
                "node {node_id}.{} emitted {:?}, expected {:?}",
                port.name,
                value.data_type(),
                port.data_type
            )));
        }

        // Single-reader edges and terminal capture can take ownership without
        // allocating a new Tensor/Vec. Fan-out clones only for extra readers.
        if let Some((last, fanout)) = senders[port_index].split_last() {
            for sender in fanout {
                send_frame(
                    node_id,
                    port.name,
                    sender,
                    Frame {
                        sequence,
                        value: value.clone(),
                        attempt: None,
                    },
                    context.cancelled,
                    context.transport,
                    context.edge_store,
                )?;
            }
            send_frame(
                node_id,
                port.name,
                last,
                Frame {
                    sequence,
                    value,
                    attempt: None,
                },
                context.cancelled,
                context.transport,
                context.edge_store,
            )?;
        } else {
            let key = format!("{node_id}.{}", port.name);
            context
                .captures
                .lock()
                .expect("capture mutex poisoned")
                .entry(key)
                .or_default()
                .push(Frame {
                    sequence,
                    value,
                    attempt: None,
                });
        }
    }
    Ok(())
}

struct Worker {
    node_id: String,
    unit: Arc<dyn FlowUnit>,
    config: JsonValue,
    inputs: Vec<EdgeReceiver>,
    outputs: Vec<Vec<EdgeSender>>,
    source_values: Option<Vec<Value>>,
    captures: Captures,
    cancelled: Arc<AtomicBool>,
    profile: Option<ProfileStore>,
    trace: Option<TraceStore>,
    transport: Option<TransportStore>,
    edge_store: Option<EdgeStore>,
}

impl Worker {
    fn execute(self) -> Result<()> {
        let context = DispatchContext {
            captures: &self.captures,
            cancelled: self.cancelled.as_ref(),
            transport: self.transport.as_ref(),
            edge_store: self.edge_store.as_ref(),
        };
        if let Some(values) = self.source_values {
            for (sequence, value) in values.into_iter().enumerate() {
                if self.cancelled.load(Ordering::Acquire) {
                    break;
                }
                dispatch(
                    &self.node_id,
                    self.unit.as_ref(),
                    sequence,
                    vec![value],
                    &self.outputs,
                    &context,
                )?;
            }
            return Ok(());
        }

        loop {
            if self.cancelled.load(Ordering::Acquire) {
                break;
            }
            let first = match receive(
                &self.inputs[0],
                &self.cancelled,
                &self.node_id,
                self.transport.as_ref(),
                self.edge_store.as_ref(),
            ) {
                Some(frame) => frame,
                None => break,
            };
            let mut values = vec![first.value];
            for receiver in self.inputs.iter().skip(1) {
                match receive(
                    receiver,
                    &self.cancelled,
                    &self.node_id,
                    self.transport.as_ref(),
                    self.edge_store.as_ref(),
                ) {
                    Some(frame) if frame.sequence == first.sequence => values.push(frame.value),
                    Some(frame) => {
                        return Err(FlowError::Execution(format!(
                            "node {} got out-of-sync frames: {} vs {}",
                            self.node_id, first.sequence, frame.sequence
                        )));
                    }
                    None if self.cancelled.load(Ordering::Acquire) => return Ok(()),
                    None => {
                        return Err(FlowError::Execution(format!(
                            "node {} lost a required input stream",
                            self.node_id
                        )));
                    }
                }
            }
            let start = (self.profile.is_some() || self.trace.is_some()).then(Instant::now);
            let processed = self.unit.process_owned(values, &self.config);
            if let Some(start) = start {
                let elapsed = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                if let Some(profile) = &self.profile {
                    let mut entries = profile.lock().expect("profile mutex poisoned");
                    let sample = entries.entry(self.node_id.clone()).or_default();
                    sample.calls += 1;
                    sample.process_ns = sample.process_ns.saturating_add(elapsed);
                }
                if let Some(trace) = &self.trace {
                    let offset = start
                        .duration_since(trace.origin)
                        .as_nanos()
                        .min(u128::from(u64::MAX)) as u64;
                    trace.record(TraceEvent {
                        node_id: self.node_id.clone(),
                        sequence: first.sequence,
                        process_started_ns: offset,
                        process_duration_ns: elapsed,
                        ok: processed.is_ok(),
                    });
                }
            }
            let output = processed?;
            dispatch(
                &self.node_id,
                self.unit.as_ref(),
                first.sequence,
                output,
                &self.outputs,
                &context,
            )?;
        }
        Ok(())
    }
}

impl Runtime {
    pub fn new(registry: FlowRegistry) -> Self {
        Self { registry }
    }

    pub fn registry_mut(&mut self) -> &mut FlowRegistry {
        &mut self.registry
    }

    pub fn compile(&self, spec: &GraphSpec) -> Result<ExecutionPlan> {
        ExecutionPlan::compile(spec, &self.registry)
    }

    pub fn run(&self, spec: &GraphSpec, value: Value) -> Result<RunResult> {
        self.run_many(spec, vec![value])
    }

    /// A bounded worker/channel pipeline for finite source batches.
    ///
    /// v0.1 supports one source, one item per source sequence, and synchronized
    /// DAG fan-out/fan-in. It does not yet implement dynamic stream joins.
    pub fn run_many(&self, spec: &GraphSpec, values: Vec<Value>) -> Result<RunResult> {
        let plan = self.compile(spec)?;
        self.run_plan(&plan, values)
    }

    /// Opt-in FlowUnit process timing. Does not include input waits, dispatch,
    /// source forwarding, thread setup, or scheduling overhead.
    pub fn run_many_profiled(&self, spec: &GraphSpec, values: Vec<Value>) -> Result<ProfiledRun> {
        let plan = self.compile(spec)?;
        let store: ProfileStore = Arc::new(Mutex::new(BTreeMap::new()));
        let result = self.run_plan_inner(&plan, values, Some(Arc::clone(&store)), None, None)?;
        let node_process = store.lock().expect("profile mutex poisoned").clone();
        Ok(ProfiledRun {
            result,
            node_process,
        })
    }

    /// Opt-in, bounded per-frame process timeline. Input/output payloads are
    /// intentionally excluded. Traces are returned for successful runs only.
    pub fn run_many_traced(&self, spec: &GraphSpec, values: Vec<Value>) -> Result<TracedRun> {
        let plan = self.compile(spec)?;
        let trace = Arc::new(TraceCollector {
            origin: Instant::now(),
            buffer: Mutex::new(TraceBuffer::default()),
        });
        let result = self.run_plan_inner(&plan, values, None, Some(Arc::clone(&trace)), None)?;
        let buffer = trace.buffer.lock().expect("trace mutex poisoned");
        let mut events = buffer.events.clone();
        events.sort_by(|a, b| {
            a.process_started_ns
                .cmp(&b.process_started_ns)
                .then_with(|| a.node_id.cmp(&b.node_id))
                .then_with(|| a.sequence.cmp(&b.sequence))
        });
        Ok(TracedRun {
            result,
            events,
            dropped_events: buffer.dropped_events,
        })
    }

    /// Opt-in channel API wall-time counters per FlowUnit worker.
    /// Receive times include upstream processing and end-of-stream probes;
    /// send times include API call overhead and bounded-channel backpressure.
    /// These are not isolated queue residency or pure transport overhead.
    pub fn run_many_transport_profiled(
        &self,
        spec: &GraphSpec,
        values: Vec<Value>,
    ) -> Result<TransportProfiledRun> {
        let plan = self.compile(spec)?;
        let store: TransportStore = Arc::new(Mutex::new(BTreeMap::new()));
        let result = self.run_plan_inner(&plan, values, None, None, Some(Arc::clone(&store)))?;
        let node_transport = store.lock().expect("transport mutex poisoned").clone();
        Ok(TransportProfiledRun {
            result,
            node_transport,
        })
    }

    pub fn run_plan(&self, plan: &ExecutionPlan, values: Vec<Value>) -> Result<RunResult> {
        self.run_plan_inner(plan, values, None, None, None)
    }

    fn run_plan_inner(
        &self,
        plan: &ExecutionPlan,
        values: Vec<Value>,
        profile: Option<ProfileStore>,
        trace: Option<TraceStore>,
        transport: Option<TransportStore>,
    ) -> Result<RunResult> {
        let expected = plan.nodes[plan.source].unit.outputs()[0].data_type;
        for (index, value) in values.iter().enumerate() {
            if value.data_type() != expected {
                return Err(FlowError::Execution(format!(
                    "source frame {index} is {:?}, expected {expected:?}",
                    value.data_type()
                )));
            }
        }
        let count = values.len();
        let cancelled = Arc::new(AtomicBool::new(false));
        let errors: FirstError = Arc::new(Mutex::new(None));
        let captures: Captures = Arc::new(Mutex::new(BTreeMap::new()));

        {
            let mut entries = captures.lock().expect("capture mutex poisoned");
            for name in plan.terminal_names() {
                entries.insert(name, Vec::new());
            }
        }

        let mut inbound: Vec<Vec<Option<Receiver<Frame>>>> = plan
            .nodes
            .iter()
            .map(|node| vec![None; node.unit.inputs().len()])
            .collect();
        let mut outbound: Vec<Vec<Vec<Sender<Frame>>>> = plan
            .nodes
            .iter()
            .map(|node| vec![Vec::new(); node.unit.outputs().len()])
            .collect();

        for edge in &plan.edges {
            let (sender, receiver) = bounded(plan.queue_capacity);
            inbound[edge.to_node][edge.to_port] = Some(receiver);
            outbound[edge.from_node][edge.from_port].push(sender);
        }

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(plan.nodes.len());
            let mut source_values = Some(values);
            for (index, node) in plan.nodes.iter().enumerate() {
                let inputs: Vec<_> = inbound[index]
                    .iter_mut()
                    .map(|receiver| receiver.take().expect("validated input is connected"))
                    .collect();
                let worker = Worker {
                    node_id: node.id.clone(),
                    unit: Arc::clone(&node.unit),
                    config: node.config.clone(),
                    inputs,
                    outputs: std::mem::take(&mut outbound[index]),
                    source_values: if index == plan.source {
                        source_values.take()
                    } else {
                        None
                    },
                    captures: Arc::clone(&captures),
                    cancelled: Arc::clone(&cancelled),
                    profile: profile.clone(),
                    trace: trace.clone(),
                    transport: transport.clone(),
                };
                let failing_node_id = node.id.clone();
                let worker_errors = Arc::clone(&errors);
                let worker_cancelled = Arc::clone(&cancelled);
                handles.push(scope.spawn(move || {
                    if let Err(error) = worker.execute() {
                        remember_error(
                            &worker_errors,
                            &worker_cancelled,
                            FlowError::NodeExecution {
                                node_id: failing_node_id,
                                message: error.to_string(),
                            },
                        );
                    }
                }));
            }
            for handle in handles {
                if handle.join().is_err() {
                    remember_error(
                        &errors,
                        &cancelled,
                        FlowError::Execution("worker thread panicked".to_owned()),
                    );
                }
            }
        });

        if let Some(error) = errors.lock().expect("error mutex poisoned").take() {
            return Err(error);
        }
        let mut outputs = BTreeMap::new();
        let mut entries = captures.lock().expect("capture mutex poisoned");
        for (name, frames) in entries.iter_mut() {
            frames.sort_by_key(|frame| frame.sequence);
            outputs.insert(
                name.clone(),
                frames.drain(..).map(|frame| frame.value).collect(),
            );
        }
        Ok(RunResult {
            outputs,
            input_count: count,
        })
    }
}
