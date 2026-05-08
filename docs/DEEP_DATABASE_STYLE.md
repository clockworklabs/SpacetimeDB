# Deep Database Style

> Inspired by [TIGER STYLE](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md).

This document records the principles by which we design the **deep core** of the SpacetimeDB database.

It is almost impossible to list every constraint the deep core must satisfy. We have begun to enumerate them, but the list is unbounded. What we can do is write down the principles by which we design the core. Principles compose. Constraints do not.

## Scope

The deep core is the part of the system on which we rely most strongly for performance and correctness. It comprises:

1. The datastore (including indexes)
2. The commitlog
3. Snapshotting
4. Replication

The principles below apply with full force inside the deep core. They may be relaxed outside it (CLI, codegen, dashboards, language SDKs, host glue), but we do not relax them inside.

## Why principles

We are designing SpacetimeDB's core from first principles. We need to own, control, and understand it. That means anything where we strongly rely on performance and correctness.

The seven principles below are what we adopt for that core. Several are written as "work towards," because we do not yet meet them everywhere. They are aspirational in scope, not in authority. When we make design decisions for the deep core, these are the principles we measure them against.

## 1. Work towards zero dependencies

Dependencies are a safety and performance risk. They lead to larger builds, longer build times, and platform portability issues, pain we have already paid for repeatedly.

We also need to know how the system behaves when we exhaust resources like disk and memory. External dependencies in the core take that control away from us. We cannot reason about a failure mode we did not write.

We do not aim to eliminate every dependency immediately. We are resolved to minimize them. Adding new dependencies is undesirable, and every additional dependency moves us further from the goal, so any new dependency must be reviewed with extreme scrutiny. The default answer to "should we add a dependency to the deep core?" is no.

Leniency may be granted for purely in-memory, `no_std` libraries that perform pure computation (Blake3, for example). These do not interact with the outside world, do not allocate, and do not affect the failure modes we are trying to control.

## 2. Work towards deterministic simulation testing

Deterministic simulation testing (DST) is the practice of running the core inside an in-memory simulator that controls every input it observes (time, randomness, I/O, message arrivals, peer behavior) and that produces the same trace given the same seed. The simulator can inject failures, reorderings, latencies, and resource exhaustion at will, and any bug it discovers can be reproduced exactly by replaying the seed.

We want this because the state space of failure behaviors in a distributed database is far too large to think through by hand. Disk corruption, partial writes, message reordering, network partitions, peer crashes, slow peers, fsync stalls: these conditions compose combinatorially with each other and with the system's own state. We cannot enumerate them, but a deterministic simulator can explore them at scale, mechanically. The choice is between encountering correctness issues in tests, on a developer's machine, with a seed in hand, or encountering them in production, where reproduction is rare and recovery is expensive. We want the former.

This applies to performance as well. We should be able to define the performance characteristics of external systems (disk, network, peers) and test SpacetimeDB under those conditions, reproducibly. A regression that appears under simulated 10ms fsync latency is a regression we can fix; one that appears only in production is not.

To "have" deterministic simulation testing means:

- The core consumes time, randomness, and I/O only through interfaces the simulator can substitute.
- A single seed produces a single trace, end-to-end, byte-for-byte.
- The simulator can inject every interesting failure mode at every interesting boundary.
- Failing runs persist their seeds as durable artifacts so they can be replayed.

For a contributor working in the deep core, this means:

- Do not read from the OS clock. Time arrives as an input.
- Do not call OS randomness. Randomness arrives as an input.
- Do not perform real I/O. I/O is delegated to a layer the simulator can substitute.
- Do not depend on iteration order of collections that do not define one (the default `HashMap`, for example).
- Do not introduce Tokio or any runtime that schedules work outside our reach (see principle 4).
- Do not spawn threads or tasks that the simulator does not own.

Determinism is what makes simulation useful. A non-deterministic bug found once is a bug we will not find again.

## 3. Work towards thread-per-core

Cache effects dominate at the time scales we care about, and context switches are expensive at our performance requirements. We have more information about our workloads than the OS scheduler does. We know what data each unit of work will touch, so we should control the scheduling of work to take advantage of cache structure.

Thread-per-core is the model that makes this possible. It gives us locality, predictability, and the ability to reason about what is running where.

## 4. Work towards `no_std`

To control our failure modes, we should enforce no memory allocation inside the core. This is not absolute. Primitives like pages can be allocated outside the core and passed in. But the rule is that the deep core does not allocate.

This is intrusive in the datastore, and we expect it to be. We cannot reach the failure-mode control we want without that intrusion. These goals and guidelines exist precisely so that resource exhaustion is something we can reason about at every call site, not something the system encounters silently.

This naturally precludes Tokio inside the core, which is desirable anyway. It serves principles 1, 2, and 3 simultaneously.

## 5. Think in terms of persistent data structures

We want to support time-travel APIs, sub-transactions, background snapshotting, and potentially MVCC. Persistent data structures, such as Merkle trees and Postgres-style MVCC, naturally allow us to look at multiple versions of data and update versions atomically.

This principle is about the externally observable behavior of the system, not a ban on mutable internals. Individual components may use mutable, non-persistent structures where that is the right tool. What matters is that the system as a whole presents the properties of a persistent data structure: prior versions remain observable, updates are atomic with respect to readers, and history is not silently overwritten.

Merkle trees are particularly valuable because, in addition to being a persistent immutable data structure, they verify integrity: each node is identified by the hash of its contents, so corruption or tampering is detectable. This comes at a performance cost, and we must weigh that cost carefully wherever we apply them.

This capability is foundational. It is much easier to design persistent structures in from the start than to retrofit them later. Unreferenced versions can always be garbage collected.

## 6. Think in terms of pipelining

We always want to decouple latency from throughput where it is possible. The principle of pipelining is that we do not wait for one operation to fully complete before beginning the next. Each operation may still take its full latency to finish, but the system as a whole keeps moving.

In the commitlog, every client must still wait for the fsync of its own messages: that is what durability means. What pipelining buys us is that the commitlog continues to process other messages while any individual client waits. Throughput is not bounded by the latency of any single fsync.

The principle generalizes. Two-phase commit, disk I/O, replication, and any place where one operation could otherwise block the start of the next are candidates.

This is a principle, not an optimization, because pipelining cannot be cleanly retrofitted. Once a system is in place, code paths assume they can call into the next operation and wait for the result, and those assumptions accumulate everywhere. Removing them later means changing call sites, error handling, and invariants throughout. The only reliable way to get pipelining is to design for it from first principles, even where the immediate workload does not yet demand it.

## 7. Think in terms of unreliable processes

We should model the core's communication with the outside world (Tokio, disk I/O, networking, peers) as unreliable, asynchronous message passing.

This sharpens our error handling. Every message can be lost, delayed, reordered, or corrupted, and the core's logic must remain correct under those conditions. Corruption is included deliberately: bits flip on disk, in transit, and in memory (cosmic rays and ordinary hardware faults alike). The core must assume that any byte it reads back may differ from the byte it wrote, and verify integrity at the boundaries where it matters. This is one of the reasons we lean on Merkle structures in principle 5.

This is also a natural fit with principle 6, since messages to other processes are inherently pipelined.

## Style

The seven principles describe how we design the deep core. The notes below describe how we write code inside it. They are inspired by TIGER STYLE, narrowed and adapted for Rust and for the principles above.

### Assertions

Assertions detect programmer errors. They close the gap between the model in our heads and the model the code actually implements.

- Assert preconditions, postconditions, and invariants. We aim for at least two assertions per function on average.
- Pair assertions across boundaries. If a property must hold, check it on at least two distinct code paths (for example, before writing to disk and again after reading back).
- Assert both the positive space (what should hold) and the negative space (what must not). The interesting bugs live at the boundary.
- Prefer `assert!(a); assert!(b);` to `assert!(a && b)` so failures are precise.
- Use `const _: () = assert!(...)` for invariants between compile-time constants and type sizes. The cheapest feedback is feedback the compiler gives you.

### Bounded everything

- Every loop has a static upper bound. If a loop must not terminate (an event loop, for example), that fact is itself asserted.
- Every queue has a fixed capacity. The deep core does not allocate to absorb load.
- No recursion in the deep core.

### Error handling

The majority of catastrophic failures in distributed systems come from the mishandling of errors that the system already knew about. Every `Result` in the deep core has a planned response: handle it, propagate it, or assert that it cannot happen and explain why. `unwrap`, `expect`, and `panic!` belong only at points where the failure is genuinely impossible by construction, and that construction must be visible at the call site.

### Control flow

Prefer simple, explicit control flow. Avoid macros where a function will do: macros obscure types, complicate tooling, and make control flow harder to follow at the call site.

### Naming

- `snake_case` for functions, variables, modules, and files.
- `CamelCase` for types, with acronyms capitalized as words per Rust convention (`VsrState`, not `VSRState`).
- Do not abbreviate. The cost of typing a long name is paid once; the cost of misreading a short one is paid forever.
- Put units and qualifiers last, in descending significance: `latency_ms_max`, not `max_latency_ms`. Related variables then line up in the source.

### Comments and formatting

- Comments should primarily explain *why*, not *what*. The code already says *what*. *What*-comments tend to drift out of sync with the code they describe and become actively misleading; we have had recent post-mortems on exactly this failure mode.
- An exception is summarizing genuinely complex logic, where a short *what*-paragraph at the top of a section lets a reader skip the body when it is not relevant to their task. Use these sparingly and keep them at a level of abstraction that is unlikely to need updating when the implementation changes.
- Run `rustfmt` and `clippy`. 100-column line limit.
- Always brace `if` bodies, even single-line, as defense in depth.

---

As we learn, and as we make these principles operational in code, we will extend this document with the practices that put each principle into action.
