use alloc::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    vec::Vec,
};
use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};

use spin::Mutex;

use super::probability::sample_duration_between;
use super::rng::Ratio;
use super::{time::TimeHandle, NodeId, Rng};

/// Directed network path from one simulated node to another.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Path {
    from: NodeId,
    to: NodeId,
}

impl Path {
    fn new(from: NodeId, to: NodeId) -> Self {
        Self { from, to }
    }

    fn crosses_partition(self, left: &BTreeSet<NodeId>) -> bool {
        self.from != self.to && left.contains(&self.from) != left.contains(&self.to)
    }
}

/// Per-runtime simulated network options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    /// Minimum simulated one-way packet delay.
    pub one_way_delay_min: Duration,
    /// Mean simulated one-way packet delay.
    pub one_way_delay_mean: Duration,
    /// Probability that the network drops an outbound packet.
    pub packet_loss_probability: Ratio,
    /// Maximum number of in-flight packets on a single directed path.
    pub path_maximum_capacity: usize,
    /// Mean simulated duration for an automatically clogged path.
    pub path_clog_duration_mean: Duration,
    /// Probability that a send automatically clogs its directed path.
    pub path_clog_probability: Ratio,
    /// Probability that the network duplicates an outbound packet.
    pub packet_replay_probability: Ratio,
    /// Probability that an unpartitioned network enters a partitioned state on a tick.
    pub partition_probability: Ratio,
    /// Probability that a newly created automatic partition blocks only one direction.
    pub one_way_partition_probability: Ratio,
    /// Probability that an automatically partitioned network heals on a tick.
    pub unpartition_probability: Ratio,
    /// Minimum ticks an automatic partition remains stable before healing is sampled.
    pub partition_stability_ticks: u32,
    /// Minimum ticks an automatically healed network remains stable before partitioning is sampled.
    pub unpartition_stability_ticks: u32,
}

const DEFAULT_ONE_WAY_DELAY_MIN: Duration = Duration::from_millis(1);
const DEFAULT_ONE_WAY_DELAY_MEAN: Duration = Duration::from_millis(10);
const DEFAULT_PATH_MAXIMUM_CAPACITY: usize = 1024;
const DEFAULT_PATH_CLOG_DURATION_MEAN: Duration = Duration::from_millis(100);

impl Default for Options {
    fn default() -> Self {
        Self {
            one_way_delay_min: DEFAULT_ONE_WAY_DELAY_MIN,
            one_way_delay_mean: DEFAULT_ONE_WAY_DELAY_MEAN,
            packet_loss_probability: Ratio::ZERO,
            path_maximum_capacity: DEFAULT_PATH_MAXIMUM_CAPACITY,
            path_clog_duration_mean: DEFAULT_PATH_CLOG_DURATION_MEAN,
            path_clog_probability: Ratio::ZERO,
            packet_replay_probability: Ratio::ZERO,
            partition_probability: Ratio::ZERO,
            one_way_partition_probability: Ratio::ZERO,
            unpartition_probability: Ratio::ZERO,
            partition_stability_ticks: 0,
            unpartition_stability_ticks: 0,
        }
    }
}

/// Shared deterministic network state for one simulation runtime.
#[derive(Clone, Debug)]
pub struct Network {
    inner: Arc<Mutex<NetworkState>>,
    rng: Rng,
    time: TimeHandle,
}

impl Network {
    pub(crate) fn new(time: TimeHandle, rng: Rng, options: Options) -> Self {
        Self {
            inner: Arc::new(Mutex::new(NetworkState::new(options))),
            rng,
            time,
        }
    }

    pub(crate) fn register_node(&self, node: NodeId) {
        self.inner.lock().nodes.entry(node).or_default();
    }

    /// Return a handle that sends from and receives for `node`.
    pub fn on_node(&self, node: NodeId) -> NodeNetwork {
        self.register_node(node);
        NodeNetwork {
            node,
            network: self.clone(),
        }
    }

    /// Clear network-owned faults while preserving process isolation, inboxes, and in-flight packets.
    pub fn clear_faults(&self) {
        let mut state = self.inner.lock();
        state.clear_node_faults();
        state.clear_link_faults();
    }

    /// Clear link-level faults while preserving node-level clogs.
    pub fn clear_link_faults(&self) {
        self.inner.lock().clear_link_faults();
    }

    /// Return whether inbound or outbound delivery is blocked for `node`.
    pub fn is_node_clogged(&self, node: NodeId) -> bool {
        self.inner.lock().is_node_clogged(node)
    }

    /// Block all inbound and outbound delivery for `node`.
    pub fn clog_node(&self, node: NodeId) {
        let mut state = self.inner.lock();
        state.clogged_node_in.insert(node);
        state.clogged_node_out.insert(node);
    }

    /// Clear the manual inbound and outbound node clog.
    pub fn unclog_node(&self, node: NodeId) {
        let mut state = self.inner.lock();
        state.clogged_node_in.remove(&node);
        state.clogged_node_out.remove(&node);
    }

    /// Isolate a paused or crashed process from all network traffic.
    pub(crate) fn isolate_node(&self, node: NodeId) {
        self.inner.lock().isolated_nodes.insert(node);
    }

    /// Remove process isolation without clearing manually configured node clogs.
    pub(crate) fn unisolate_node(&self, node: NodeId) {
        self.inner.lock().isolated_nodes.remove(&node);
    }

    /// Block all inbound delivery to `node`.
    pub fn clog_node_in(&self, node: NodeId) {
        self.inner.lock().clogged_node_in.insert(node);
    }

    /// Clear the manual inbound node clog.
    pub fn unclog_node_in(&self, node: NodeId) {
        self.inner.lock().clogged_node_in.remove(&node);
    }

    /// Block all outbound delivery from `node`.
    pub fn clog_node_out(&self, node: NodeId) {
        self.inner.lock().clogged_node_out.insert(node);
    }

    /// Clear the manual outbound node clog.
    pub fn unclog_node_out(&self, node: NodeId) {
        self.inner.lock().clogged_node_out.remove(&node);
    }

    /// Block delivery from `from` to `to`.
    pub fn clog_link(&self, from: NodeId, to: NodeId) {
        self.inner.lock().clogged_links.insert(Path::new(from, to));
    }

    /// Clear the manual directed-link clog.
    pub fn unclog_link(&self, from: NodeId, to: NodeId) {
        self.inner.lock().clogged_links.remove(&Path::new(from, to));
    }

    /// Return whether a payload from `from` to `to` would currently be blocked.
    pub fn is_blocked(&self, from: NodeId, to: NodeId) -> bool {
        let mut state = self.inner.lock();
        state.expire_path_clogs(self.time.now());
        state.is_path_blocked(Path::new(from, to))
    }

    /// Enqueue one payload from `from` to `to` into the simulated network.
    pub fn send(&self, from: NodeId, to: NodeId, payload: Vec<u8>) -> Result<(), SendError> {
        let now = self.time.now();
        self.inner.lock().send(Path::new(from, to), now, payload, &self.rng)
    }

    /// Drain simulated network deliveries ready at `now`.
    pub(crate) fn tick(&self, now: Duration) -> bool {
        let Some(wakers) = self.inner.lock().tick(now, &self.rng) else {
            return false;
        };
        for waker in wakers {
            waker.wake();
        }
        true
    }

    pub(crate) fn next_delivery_deadline(&self, now: Duration) -> Option<Duration> {
        self.inner.lock().next_delivery_deadline(now)
    }

    /// Receive one payload addressed to `node`.
    pub fn recv(&self, node: NodeId) -> Recv {
        self.register_node(node);
        Recv {
            node,
            network: self.clone(),
        }
    }
}

/// Per-node network handle.
#[derive(Clone, Debug)]
pub struct NodeNetwork {
    node: NodeId,
    network: Network,
}

impl NodeNetwork {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn send(&self, to: NodeId, payload: Vec<u8>) -> Result<(), SendError> {
        self.network.send(self.node, to, payload)
    }

    pub fn recv(&self) -> Recv {
        self.network.recv(self.node)
    }

    pub fn network(&self) -> &Network {
        &self.network
    }
}

/// One delivered simulated network payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Packet {
    pub from: NodeId,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PacketEvent {
    path: Path,
    deliver_at: Duration,
    payload: Vec<u8>,
}

/// Error returned when the simulated network refuses a send submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendError {
    UnknownNode { node: NodeId },
    PathAtCapacity { from: NodeId, to: NodeId, capacity: usize },
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode { node } => write!(f, "simulated network node {node} is not registered"),
            Self::PathAtCapacity { from, to, capacity } => {
                write!(f, "simulated network path {from}->{to} reached capacity {capacity}")
            }
        }
    }
}

#[derive(Debug)]
struct NetworkState {
    options: Options,
    nodes: BTreeMap<NodeId, Inbox>,
    /// Packets accepted by the network but not delivered yet.
    ///
    /// Delayed and blocked packets both stay here so path capacity accounting
    /// and random delivery order use a single source of truth.
    in_flight: VecDeque<PacketEvent>,
    /// Node-level manual faults. These have no local deadline.
    clogged_node_in: BTreeSet<NodeId>,
    clogged_node_out: BTreeSet<NodeId>,
    /// Executor-owned node isolation for paused or crashed processes.
    isolated_nodes: BTreeSet<NodeId>,
    /// Link-level manual faults. These have no local deadline.
    clogged_links: BTreeSet<Path>,
    /// Link faults created by the automatic partition state machine.
    automatic_partition_links: BTreeSet<Path>,
    /// Temporary directed-path clogs keyed by their expiry time.
    path_clogs: BTreeMap<Path, Duration>,
    partition_stability_ticks_remaining: u32,
    unpartition_stability_ticks_remaining: u32,
}

impl NetworkState {
    fn new(options: Options) -> Self {
        Self {
            options,
            nodes: BTreeMap::new(),
            in_flight: VecDeque::new(),
            clogged_node_in: BTreeSet::new(),
            clogged_node_out: BTreeSet::new(),
            isolated_nodes: BTreeSet::new(),
            clogged_links: BTreeSet::new(),
            automatic_partition_links: BTreeSet::new(),
            path_clogs: BTreeMap::new(),
            partition_stability_ticks_remaining: 0,
            unpartition_stability_ticks_remaining: 0,
        }
    }

    /// Clear manually configured node-level network faults.
    fn clear_node_faults(&mut self) {
        self.clogged_node_in.clear();
        self.clogged_node_out.clear();
    }

    /// Clear faults owned by the network path and partition machinery.
    fn clear_link_faults(&mut self) {
        self.clogged_links.clear();
        self.automatic_partition_links.clear();
        self.path_clogs.clear();
        self.partition_stability_ticks_remaining = 0;
        self.unpartition_stability_ticks_remaining = 0;
    }

    /// Return whether any node-level fault blocks inbound or outbound traffic.
    fn is_node_clogged(&self, node: NodeId) -> bool {
        self.clogged_node_in.contains(&node)
            || self.clogged_node_out.contains(&node)
            || self.isolated_nodes.contains(&node)
    }

    /// Accept one outbound packet into the simulated network.
    ///
    /// Capacity is checked before loss/replay so a dropped packet still
    /// observes the same backpressure as a real send attempt. Accepted packets
    /// stay in `in_flight` until `tick` moves them to an inbox.
    fn send(&mut self, path: Path, now: Duration, payload: Vec<u8>, rng: &Rng) -> Result<(), SendError> {
        self.expire_path_clogs(now);

        for node in [path.from, path.to] {
            if !self.nodes.contains_key(&node) {
                return Err(SendError::UnknownNode { node });
            }
        }

        let options = self.options;

        let path_capacity = options.path_maximum_capacity;
        if self.path_in_flight(path) >= path_capacity {
            return Err(SendError::PathAtCapacity {
                from: path.from,
                to: path.to,
                capacity: path_capacity,
            });
        }

        if rng.buggify_ratio(options.packet_loss_probability) {
            return Ok(());
        }

        if rng.buggify_ratio(options.path_clog_probability) {
            self.clog_path(path, now, rng);
        }

        let deliver_at = now.saturating_add(sample_duration_between(
            rng,
            options.one_way_delay_min,
            options.one_way_delay_mean,
        ));

        if rng.buggify_ratio(options.packet_replay_probability)
            && self.path_in_flight(path).saturating_add(1) < path_capacity
        {
            self.in_flight.push_back(PacketEvent {
                path,
                deliver_at,
                payload: payload.clone(),
            });
        }
        self.in_flight.push_back(PacketEvent {
            path,
            deliver_at,
            payload,
        });
        Ok(())
    }

    /// Run one network tick at the current virtual time.
    ///
    /// A tick delivers packets already ready at this instant, advances partition
    /// state, then drains again for packets unblocked by a heal. This prevents a
    /// newly created partition from retroactively blocking packets whose
    /// `deliver_at` is already due.
    fn tick(&mut self, now: Duration, rng: &Rng) -> Option<Vec<Waker>> {
        self.expire_path_clogs(now);

        let mut wakers = Vec::new();
        let mut delivered = self.drain_deliverable_packets(now, rng, &mut wakers);
        self.maybe_update_partition(rng);
        delivered |= self.drain_deliverable_packets(now, rng, &mut wakers);

        delivered.then_some(wakers)
    }

    /// Move every currently deliverable packet to its destination inbox.
    ///
    /// Delivery order remains randomized, but choosing each packet uses
    /// reservoir sampling over the ready subset instead of allocating a list.
    fn drain_deliverable_packets(&mut self, now: Duration, rng: &Rng, wakers: &mut Vec<Waker>) -> bool {
        let mut delivered = false;
        while let Some(index) = self.deliverable_packet_index(rng, now) {
            delivered = true;
            let event = self.in_flight.remove(index).expect("index came from in-flight queue");
            let inbox = self.nodes.entry(event.path.to).or_default();
            inbox.messages.push_back(Packet {
                from: event.path.from,
                payload: event.payload,
            });
            if let Some(waker) = inbox.waker.take() {
                wakers.push(waker);
            }
        }
        delivered
    }

    /// Count all packets occupying capacity on one directed path.
    fn path_in_flight(&self, path: Path) -> usize {
        self.in_flight.iter().filter(|event| event.path == path).count()
    }

    /// Add a temporary clog whose lifetime is driven by virtual time.
    fn clog_path(&mut self, path: Path, now: Duration, rng: &Rng) {
        let duration = sample_duration_between(rng, Duration::ZERO, self.options.path_clog_duration_mean);
        if duration.is_zero() {
            return;
        }
        let until = now.saturating_add(duration);
        self.path_clogs
            .entry(path)
            .and_modify(|existing| *existing = (*existing).max(until))
            .or_insert(until);
    }

    /// Drop expired temporary path clogs before delivery or deadline checks.
    fn expire_path_clogs(&mut self, now: Duration) {
        self.path_clogs.retain(|_, until| *until > now);
    }

    /// Return whether any current network fault blocks a directed path.
    fn is_path_blocked(&self, path: Path) -> bool {
        self.is_path_blocked_until_external_change(path) || self.path_clogs.contains_key(&path)
    }

    /// Return whether a path is blocked by a fault with no packet-local expiry.
    fn is_path_blocked_until_external_change(&self, path: Path) -> bool {
        self.clogged_node_out.contains(&path.from)
            || self.clogged_node_in.contains(&path.to)
            || self.isolated_nodes.contains(&path.from)
            || self.isolated_nodes.contains(&path.to)
            || self.clogged_links.contains(&path)
            || self.automatic_partition_links.contains(&path)
    }

    /// Choose one ready packet uniformly from the ready subset.
    fn deliverable_packet_index(&self, rng: &Rng, now: Duration) -> Option<usize> {
        let mut selected = None;
        let mut ready = 0;
        for (index, event) in self.in_flight.iter().enumerate() {
            if event.deliver_at > now || self.is_path_blocked(event.path) {
                continue;
            }
            ready += 1;
            if rng.index(ready) == 0 {
                selected = Some(index);
            }
        }
        selected
    }

    /// Return the next packet/path-clog deadline at which delivery may make progress.
    fn next_delivery_deadline(&mut self, now: Duration) -> Option<Duration> {
        self.expire_path_clogs(now);
        self.in_flight
            .iter()
            .filter_map(|event| self.packet_delivery_deadline(event, now))
            .min()
    }

    /// Compute the earliest time this packet can become deliverable by itself.
    fn packet_delivery_deadline(&self, event: &PacketEvent, now: Duration) -> Option<Duration> {
        // Manual node/link clogs and active partitions have no packet-local
        // deadline; only an external mutation or a later network tick can unblock them.
        if self.is_path_blocked_until_external_change(event.path) {
            return None;
        }

        let deadline = self
            .path_clogs
            .get(&event.path)
            .copied()
            .unwrap_or(event.deliver_at)
            .max(event.deliver_at);
        (deadline > now).then_some(deadline)
    }

    /// Advance the automatic partition state machine during a network tick.
    fn maybe_update_partition(&mut self, rng: &Rng) {
        if !rng.is_buggify_enabled() || self.nodes.len() < 2 {
            return;
        }

        if self.partition_stability_ticks_remaining > 0 {
            self.partition_stability_ticks_remaining -= 1;
            return;
        }

        if !self.automatic_partition_links.is_empty() {
            if rng.buggify_ratio(self.options.unpartition_probability) {
                self.automatic_partition_links.clear();
                self.unpartition_stability_ticks_remaining = self.options.unpartition_stability_ticks;
            }
            return;
        }

        if self.unpartition_stability_ticks_remaining > 0 {
            self.unpartition_stability_ticks_remaining -= 1;
            return;
        }

        if rng.buggify_ratio(self.options.partition_probability) {
            self.create_partition(rng);
            self.partition_stability_ticks_remaining = self.options.partition_stability_ticks;
        }
    }

    /// Build a deterministic random partition over the registered nodes.
    fn create_partition(&mut self, rng: &Rng) {
        let nodes = self.nodes.keys().copied().collect::<Vec<_>>();
        if nodes.len() < 2 {
            return;
        }
        let left = random_partition_side(&nodes, rng);
        let one_way_left_to_right = self
            .options
            .one_way_partition_probability
            .sample(rng)
            .then_some(rng.index(2) == 0);

        for &from in &nodes {
            for &to in &nodes {
                let path = Path::new(from, to);
                if !path.crosses_partition(&left) {
                    continue;
                }

                let blocked = match one_way_left_to_right {
                    None => true,
                    Some(left_to_right) => left.contains(&path.from) == left_to_right,
                };
                if blocked {
                    self.automatic_partition_links.insert(path);
                }
            }
        }
    }

    /// Pop one queued packet or remember the latest receiver waker.
    fn recv(&mut self, node: NodeId, waker: &Waker) -> Poll<Packet> {
        let inbox = self.nodes.entry(node).or_default();
        if let Some(packet) = inbox.messages.pop_front() {
            Poll::Ready(packet)
        } else {
            inbox.waker = Some(waker.clone());
            Poll::Pending
        }
    }
}

/// Pick a non-empty, non-total random side in deterministic node order.
fn random_partition_side(nodes: &[NodeId], rng: &Rng) -> BTreeSet<NodeId> {
    assert!(nodes.len() >= 2, "partition requires at least two nodes");

    let mut shuffled = nodes.to_vec();
    let side_len = 1 + rng.index(nodes.len() - 1);
    for index in 0..side_len {
        let swap_with = index + rng.index(shuffled.len() - index);
        shuffled.swap(index, swap_with);
    }
    shuffled.into_iter().take(side_len).collect()
}

#[derive(Debug, Default)]
struct Inbox {
    messages: VecDeque<Packet>,
    waker: Option<Waker>,
}

pub struct Recv {
    node: NodeId,
    network: Network,
}

impl Future for Recv {
    type Output = Packet;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.network.inner.lock().recv(self.node, cx.waker())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use core::time::Duration;

    use super::super::{Node, Runtime, RuntimeConfig};
    use super::*;

    fn node_net(node: &Node) -> NodeNetwork {
        node.net().expect("test runtime should have a simulated network")
    }

    #[test]
    fn clogged_link_blocks_delivery_until_unclogged() {
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(Options::default()));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let net = handle.network().expect("test runtime should have a simulated network");
        let a_net = net.on_node(a.id());
        let b_net = net.on_node(b.id());

        runtime.block_on(async {
            net.clog_link(a.id(), b.id());
            a_net.send(b.id(), vec![1]).unwrap();

            net.unclog_link(a.id(), b.id());
            let packet = b_net.recv().await;
            assert_eq!(packet.from, a.id());
            assert_eq!(packet.payload, vec![1]);
        });
    }

    #[test]
    fn path_capacity_refuses_extra_packet() {
        let options = Options {
            path_maximum_capacity: 1,
            ..Options::default()
        };
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let a_net = node_net(&a);

        a_net.send(b.id(), vec![1]).unwrap();
        assert!(matches!(
            a_net.send(b.id(), vec![2]),
            Err(SendError::PathAtCapacity { .. })
        ));

        runtime.block_on(async {
            let packet = node_net(&b).recv().await;
            assert_eq!(packet.payload, vec![1]);
        });
    }

    #[test]
    fn path_clog_never_shortens_existing_clog() {
        let options = Options {
            path_clog_duration_mean: Duration::from_nanos(1),
            ..Options::default()
        };
        let runtime = Runtime::new(0);
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let path = Path::new(a.id(), b.id());
        let existing_until = Duration::from_secs(10);
        let mut state = NetworkState::new(options);

        state.path_clogs.insert(path, existing_until);
        state.clog_path(path, Duration::ZERO, &Rng::new(0));

        assert_eq!(state.path_clogs.get(&path).copied(), Some(existing_until));
    }

    #[test]
    fn manual_node_clog_survives_pause_resume() {
        let runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(Options::default()));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let net = handle.network().expect("test runtime should have a simulated network");

        net.clog_node(b.id());
        b.pause();
        b.resume();

        assert!(net.is_blocked(a.id(), b.id()));
        net.unclog_node(b.id());
        assert!(!net.is_blocked(a.id(), b.id()));
    }

    #[test]
    fn delayed_packet_advances_virtual_time() {
        let options = Options {
            one_way_delay_min: Duration::from_millis(5),
            one_way_delay_mean: Duration::from_millis(5),
            ..Options::default()
        };
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let a_net = node_net(&a);
        let b_net = node_net(&b);

        runtime.block_on(async {
            a_net.send(b.id(), vec![1]).unwrap();
            let packet = b_net.recv().await;
            assert_eq!(packet.payload, vec![1]);
            assert!(handle.now() >= Duration::from_millis(5));
        });
    }

    #[test]
    fn same_deadline_packets_are_delivered_before_receiver_runs() {
        let options = Options {
            one_way_delay_min: Duration::from_millis(5),
            one_way_delay_mean: Duration::from_millis(5),
            ..Options::default()
        };
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let net = handle.network().expect("test runtime should have a simulated network");
        let a_net = net.on_node(a.id());
        let b_net = net.on_node(b.id());

        runtime.block_on(async {
            a_net.send(b.id(), vec![1]).unwrap();
            a_net.send(b.id(), vec![2]).unwrap();

            let first = b_net.recv().await;
            {
                let state = net.inner.lock();
                assert!(state.in_flight.is_empty());
                assert_eq!(state.nodes.get(&b.id()).unwrap().messages.len(), 1);
            }

            let second = b_net.recv().await;
            let mut payloads = vec![first.payload, second.payload];
            payloads.sort();
            assert_eq!(payloads, vec![vec![1], vec![2]]);
        });
    }

    #[test]
    fn zero_delay_packet_sent_by_woken_task_is_delivered_without_deadlock() {
        let options = Options {
            one_way_delay_min: Duration::ZERO,
            one_way_delay_mean: Duration::ZERO,
            ..Options::default()
        };
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let a_net = node_net(&a);
        let b_net = node_net(&b);

        runtime.block_on(async {
            a_net.send(b.id(), vec![1]).unwrap();
            assert_eq!(b_net.recv().await.payload, vec![1]);

            b_net.send(a.id(), vec![2]).unwrap();
            assert_eq!(a_net.recv().await.payload, vec![2]);
        });
    }

    #[test]
    fn packet_loss_drops_packet() {
        let options = Options {
            packet_loss_probability: Ratio::new(1, 1),
            ..Options::default()
        };
        let runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        runtime.enable_buggify();
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let a_net = node_net(&a);

        a_net.send(b.id(), vec![1]).unwrap();
        assert!(handle
            .network()
            .expect("test runtime should have a simulated network")
            .inner
            .lock()
            .in_flight
            .is_empty());
    }

    #[test]
    fn buggify_disabled_does_not_drop_packet() {
        let options = Options {
            packet_loss_probability: Ratio::new(1, 1),
            ..Options::default()
        };
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let a_net = node_net(&a);
        let b_net = node_net(&b);

        a_net.send(b.id(), vec![1]).unwrap();

        runtime.block_on(async {
            let packet = b_net.recv().await;
            assert_eq!(packet.payload, vec![1]);
        });
    }

    #[test]
    fn automatic_partition_heals_blocked_packet_on_later_timer_tick() {
        let options = Options {
            one_way_delay_min: Duration::ZERO,
            one_way_delay_mean: Duration::ZERO,
            partition_probability: Ratio::new(1, 1),
            unpartition_probability: Ratio::new(1, 1),
            ..Options::default()
        };
        let mut runtime = Runtime::with_config(RuntimeConfig::new(0).with_network(options));
        runtime.enable_buggify();
        let handle = runtime.handle();
        let a = handle.create_node().build();
        let b = handle.create_node().build();
        let a_net = node_net(&a);
        let b_net = node_net(&b);
        let timer = handle.clone();
        let _timer = a.spawn(async move {
            timer.sleep(Duration::from_millis(1)).await;
        });

        runtime.block_on(async {
            a_net.send(b.id(), vec![1]).unwrap();
            assert_eq!(b_net.recv().await.payload, vec![1]);

            a_net.send(b.id(), vec![2]).unwrap();
            let packet = b_net.recv().await;
            assert_eq!(packet.payload, vec![2]);
        });
    }
}
