# Spacetime Programming Standards

> This document is inspired by and adapted from [TIGER STYLE](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md).

## General Philosophy

This document outlines the philosophy, style, and requirements for code contributed to SpacetimeDB.

There are 5 core principles for the SpacetimeDB codebase which should guide all contributors to
SpacetimeDB.

- Rust as Infrastructure
- Reliable Core
- Performance Matters 
- Well Tested
- Stive for Simplicity
- Progressively Less Bad
 
The specific programming style of SpacetimeDB is evolving, but the above principles should guide
its evolution. These principles are hard learned. 

Our design goals are correctness, performance, and developer experience, in that order.A

## Reliable Core

- Progressively expanding correct core
- No dependencies
- Don't rely on OS abstraction
- No OS allocation
- Actor model
- Testing/no regressions

SpacetimeDB is a hugely ambitious project which requires code written in several different
languages, mulitanancy, complex networking, and hosting and executing user written code. As a
result, we need to use libraries and code, which we do not ourselves fully understand, do not have
control over, and have not ourselves verified the correctness of. However, there are aspects of
SpacetimeDB's operation where correctness and safety must be inviolable. Our users depend on
SpacetimeDB to never lose data, never corrupt data, execute transactions atomically, maintain
isolation, and to be fullyc onsistent. Generally speaking this are the ACID properties of the
database.

The code which is responsible for maintaining these ACID properies is called the Reliable Core of
the database. The programming standards are heightened for the Reliable Core.

As of today the Reliable Core of the database consists of:

- Datastore
- Commitlog
- Replication

> NOTE: The datastore and commitlog collectively comprise the DatabaseEnigne

If these three subsystems operate correctly, then we can more confidently make representations
regarding the ACID properties we to promise users. It does not eliminate all bugs of course, but it
eliminates certain failure modes.

### Safety

[NASA's Power of Ten — Rules for Developing Safety Critical
Code](https://spinroot.com/gerard/pdf/P10.pdf) will change the way you code forever. To expand:

- Use **only very simple, explicit control flow** for clarity. **Do not use recursion** to ensure
  that all executions that should be bounded are bounded.

- **Put a limit on everything** because, in reality, this is what we expect—everything has a limit.
  For example, all loops and all queues must have a fixed upper bound to prevent infinite loops or
  tail latency spikes. Where a loop must not terminate (e.g. an event loop), the opposite is true,
  it must be asserted that it does not terminate.

- Use explicitly-sized types like `u32` for everything, avoid architecture-specific `usize`.

- **Assertions detect programmer errors. Unlike operating errors, which are expected and which must
  be handled, assertion failures are unexpected. The only correct way to handle corrupt code is to
  crash. Assertions downgrade catastrophic correctness bugs into liveness bugs. Assertions are a
  force multiplier for discovering bugs by fuzzing.**

  - **Assert all function arguments and return values, pre/postconditions and invariants.** A
    function must not operate blindly on data it has not checked. The purpose of a function is to
    increase the probability that a program is correct. Assertions within a function are part of how
    functions serve this purpose. The assertion density of the code must average a minimum of two
    assertions per function.

  - **[Pair assertions](https://tigerbeetle.com/blog/2023-12-27-it-takes-two-to-contract).** For
    every property you want to enforce, try to find at least two different code paths where an
    assertion can be added. For example, assert validity of data right before writing it to disk,
    and also immediately after reading from disk.

  - On occasion, you may use a blatantly true assertion instead of a comment as stronger
    documentation where the assertion condition is critical and surprising.

  - Split compound assertions: prefer `assert(a); assert(b);` over `assert(a and b);`.
    The former is simpler to read, and provides more precise information if the condition fails.

  - **Assert the relationships of compile-time constants** as a sanity check, and also to document
    and enforce subtle
    invariants
    or [type
    sizes](https://github.com/clockworklabs/SpacetimeDB/blob/48b8a31fe02f0fdb71143fa383c3d4a3fbc1e6ba/crates/table/src/indexes.rs#L60).
    Compile-time assertions are extremely powerful because they are able to check a program's design
    integrity _before_ the program even executes.

  - **The golden rule of assertions is to assert the _positive space_ that you do expect AND to
    assert the _negative space_ that you do not expect** because where data moves across the
    valid/invalid boundary between these spaces is where interesting bugs are often found. This is
    also why **tests must test exhaustively**, not only with valid data but also with invalid data,
    and as valid data becomes invalid.

  - Assertions are a safety net, not a substitute for human understanding. With simulation testing,
    there is the temptation to trust the fuzzer. But a fuzzer can prove only the presence of bugs,
    not their absence. Therefore:
    - Build a precise mental model of the code first,
    - encode your understanding in the form of assertions,
    - write the code and comments to explain and justify the mental model to your reviewer,
    - and use VOPR as the final line of defense, to find bugs in your and reviewer's understanding
      of code.

- All memory in the Reliable Core, must be allocated from the operating system at startup, and
  and managed by SpacetimeDB code thereafter. In this way, we can ensure that memory allocation is
  explicit and intentional. This avoids unpredictable behavior that can significantly affect
  performance. It also allows us to reason about data locality and heap fragmentation, since we
  know where resources are physically allocated. Note that one must always do memory allocation,
  but in this case we can reason about resource constraints more explicitly because we control all
  allocations.

- Declare variables at the **smallest possible scope**, and **minimize the number of variables in
  scope**, to reduce the probability that variables are misused.

- Appreciate, from day one, **all compiler warnings at the compiler's strictest setting**.

- Whenever your program has to interact with external entities, **don't do things directly in
  reaction to external events**. Instead, your program should run at its own pace. Not only does
  this make your program safer by keeping the control flow of your program under your control, it
  also improves performance for the same reason (you get to batch, instead of context switching on
  every event). Additionally, this makes it easier to maintain bounds on work done per time period.

Beyond these rules:

- Compound conditions that evaluate multiple booleans make it difficult for the reader to verify
  that all cases are handled. Split compound conditions into simple conditions using nested
  `if/else` branches. Split complex `else if` chains into `else { if { } }` trees. This makes the
  branches and cases clear. Again, consider whether a single `if` does not also need a matching
  `else` branch, to ensure that the positive and negative spaces are handled or asserted.

- Negations are not easy! State invariants positively. When working with lengths and indexes, this
  form is easy to get right (and understand):

  ```zig
  if (index < length) {
    // The invariant holds.
  } else {
    // The invariant doesn't hold.
  }
  ```

  This form is harder, and also goes against the grain of how `index` would typically be compared to
  `length`, for example, in a loop condition:

  ```zig
  if (index >= length) {
    // It's not true that the invariant holds.
  }
  ```

- All errors must be handled. An [analysis of production failures in distributed data-intensive
  systems](https://www.usenix.org/system/files/conference/osdi14/osdi14-paper-yuan.pdf) found that
  the majority of catastrophic failures could have been prevented by simple testing of error
  handling code.

> “Specifically, we found that almost all (92%) of the catastrophic system failures are the result
> of incorrect handling of non-fatal errors explicitly signaled in software.”

- **Always motivate, always say why**. Never forget to say why. Because if you explain the rationale
  for a decision, it not only increases the hearer's understanding, and makes them more likely to
  adhere or comply, but it also shares criteria with them with which to evaluate the decision and
  its importance.

- **Explicitly pass options to library functions at the call site, instead of relying on the
  defaults**. For example, write `@prefetch(a, .{ .cache = .data, .rw = .read, .locality = 3 });`
  over `@prefetch(a, .{});`. This improves readability but most of all avoids latent, potentially
  catastrophic bugs in case the library ever changes its defaults.


## Rust as Infrastructure

- SpacetimeDB is written in Rust
- Easier when all contributors know the same language
- Infrastructure is checked into the repo
- Everyone knows how to run all tests
- Testing is repeatable on all machines

## Performance Matters

- No optimization without measurement
- Work to fanfare ratio
- One thread/one core
- No OS allocation

## Well Tested
- Testin

## Strive for Simplicity 

- Think first, design second, program third
- Easiest thing to explain to someone who has no idea what's going on
- Strive for simple infrastructure (single executable)
  - Ease of operation (easier to run a single executable)
  - Ease of understanding 
- Single purpose for different systems
- Programmable everything

## Style Guide

- Optimize for readability (avoid macros)
- 

## On Simplicity And Elegance

Simplicity is not a free pass. It's not in conflict with our design goals. It need not be a
concession or a compromise.

Rather, simplicity is how we bring our design goals together, how we identify the “super idea” that
solves the axes simultaneously, to achieve something elegant.

> “Simplicity and elegance are unpopular because they require hard work and discipline to achieve” —
> Edsger Dijkstra

Contrary to popular belief, simplicity is also not the first attempt but the hardest revision. It's
easy to say “let's do something simple”, but to do that in practice takes thought, multiple passes,
many sketches, and still we may have to [“throw one
away”](https://en.wikipedia.org/wiki/The_Mythical_Man-Month).

The hardest part, then, is how much thought goes into everything.

We spend this mental energy upfront, proactively rather than reactively, because we know that when
the thinking is done, what is spent on the design will be dwarfed by the implementation and testing,
and then again by the costs of operation and maintenance.

An hour or day of design is worth weeks or months in production:

> “the simple and elegant systems tend to be easier and faster to design and get right, more
> efficient in execution, and much more reliable” — Edsger Dijkstra

## Technical Debt

What could go wrong? What's wrong? Which question would we rather ask? The former, because code,
like steel, is less expensive to change while it's hot. A problem solved in production is many times
more expensive than a problem solved in implementation, or a problem solved in design.

Since it's hard enough to discover showstoppers, when we do find them, we solve them. We don't allow
potential memcpy latency spikes, or exponential complexity algorithms to slip through.

> “You shall not pass!” — Gandalf

In other words, TigerBeetle has a “zero technical debt” policy. We do it right the first time. This
is important because the second time may not transpire, and because doing good work, that we can be
proud of, builds momentum.

We know that what we ship is solid. We may lack crucial features, but what we have meets our design
goals. This is the only way to make steady incremental progress, knowing that the progress we have
made is indeed progress.


## Performance

> “The lack of back-of-the-envelope performance sketches is the root of all evil.” — Rivacindela
> Hudsoni

- Think about performance from the outset, from the beginning. **The best time to solve performance,
  to get the huge 1000x wins, is in the design phase, which is precisely when we can't measure or
  profile.** It's also typically harder to fix a system after implementation and profiling, and the
  gains are less. So you have to have mechanical sympathy. Like a carpenter, work with the grain.

- **Perform back-of-the-envelope sketches with respect to the four resources (network, disk, memory,
  CPU) and their two main characteristics (bandwidth, latency).** Sketches are cheap. Use sketches
  to be “roughly right” and land within 90% of the global maximum.

- Optimize for the slowest resources first (network, disk, memory, CPU) in that order, after
  compensating for the frequency of usage, because faster resources may be used many times more. For
  example, a memory cache miss may be as expensive as a disk fsync, if it happens many times more.

- Distinguish between the control plane and data plane. A clear delineation between control plane
  and data plane through the use of batching enables a high level of assertion safety without losing
  performance. See our [July 2021 talk on Zig SHOWTIME](https://youtu.be/BH2jvJ74npM?t=1958) for
  examples.

- Amortize network, disk, memory and CPU costs by batching accesses.

- Let the CPU be a sprinter doing the 100m. Be predictable. Don't force the CPU to zig zag and
  change lanes. Give the CPU large enough chunks of work. This comes back to batching.

- Be explicit. Minimize dependence on the compiler to do the right thing for you.

  In particular, extract hot loops into stand-alone functions with primitive arguments without
  `self` (see [an example](https://github.com/tigerbeetle/tigerbeetle/blob/0.16.19/src/lsm/compaction.zig#L1932-L1937)).
  That way, the compiler doesn't need to prove that it can cache struct's fields in registers, and a
  human reader can spot redundant computations easier.

## Developer Experience

> “There are only two hard things in Computer Science: cache invalidation, naming things, and
> off-by-one errors.” — Phil Karlton

### Naming Things

- **Get the nouns and verbs just right.** Great names are the essence of great code, they capture
  what a thing is or does, and provide a crisp, intuitive mental model. They show that you
  understand the domain. Take time to find the perfect name, to find nouns and verbs that work
  together, so that the whole is greater than the sum of its parts.

- Use `snake_case` for function, variable, and file names. The underscore is the closest thing we
  have as programmers to a space, and helps to separate words and encourage descriptive names. We
  don't use Zig's `CamelCase.zig` style for "struct" files to keep the convention simple and
  consistent.

- Do not abbreviate variable names, unless the variable is a primitive integer type used as an
  argument to a sort function or matrix calculation. Use long form arguments in scripts: `--force`,
  not `-f`. Single letter flags are for interactive usage.

- Use proper capitalization for acronyms (`VSRState`, not `VsrState`).

- For the rest, follow the Zig style guide.

- Add units or qualifiers to variable names, and put the units or qualifiers last, sorted by
  descending significance, so that the variable starts with the most significant word, and ends with
  the least significant word. For example, `latency_ms_max` rather than `max_latency_ms`. This will
  then line up nicely when `latency_ms_min` is added, as well as group all variables that relate to
  latency.

- Infuse names with meaning. For example, `allocator: Allocator` is a good, if boring name,
  but `gpa: Allocator` and `arena: Allocator` are excellent. They inform the reader whether
  `deinit` should be called explicitly.

- When choosing related names, try hard to find names with the same number of characters so that
  related variables all line up in the source. For example, as arguments to a memcpy function,
  `source` and `target` are better than `src` and `dest` because they have the second-order effect
  that any related variables such as `source_offset` and `target_offset` will all line up in
  calculations and slices. This makes the code symmetrical, with clean blocks that are easier for
  the eye to parse and for the reader to check.

- When a single function calls out to a helper function or callback, prefix the name of the helper
  function with the name of the calling function to show the call history. For example,
  `read_sector()` and `read_sector_callback()`.

- Callbacks go last in the list of parameters. This mirrors control flow: callbacks are also
  _invoked_ last.

- _Order_ matters for readability (even if it doesn't affect semantics). On the first read, a file
  is read top-down, so put important things near the top. The `main` function goes first.

  The same goes for `structs`, the order is fields then types then methods:

  ```zig
  time: Time,
  process_id: ProcessID,

  const ProcessID = struct { cluster: u128, replica: u8 };
  const Tracer = @This(); // This alias concludes the types section.

  pub fn init(gpa: std.mem.Allocator, time: Time) !Tracer {
      ...
  }
  ```

  If a nested type is complex, make it a top-level struct.

  At the same time, not everything has a single right order. When in doubt, consider sorting
  alphabetically, taking advantage of big-endian naming.

- Don't overload names with multiple meanings that are context-dependent. For example, TigerBeetle
  has a feature called _pending transfers_ where a pending transfer can be subsequently _posted_ or
  _voided_. At first, we called them _two-phase commit transfers_, but this overloaded the
  _two-phase commit_ terminology that was used in our consensus protocol, causing confusion.

- Think of how names will be used outside the code, in documentation or communication. For example,
  a noun is often a better descriptor than an adjective or present participle, because a noun can be
  directly used in correspondence without having to be rephrased. Compare `replica.pipeline` vs
  `replica.preparing`. The former can be used directly as a section header in a document or
  conversation, whereas the latter must be clarified. Noun names compose more clearly for derived
  identifiers, e.g. `config.pipeline_max`.

- Zig has named arguments through the `options: struct` pattern. Use it when arguments can be
  mixed up. A function taking two `u64` must use an options struct. If an argument can be `null`,
  it should be named so that the meaning of `null` literal at the call site is clear.

  Because dependencies like an allocator or a tracer are singletons with unique types, they should
  be threaded through constructors positionally, from the most general to the most specific.

- **Write descriptive commit messages** that inform and delight the reader, because your commit
  messages are being read.

- Don't forget to say why. Code alone is not documentation. Use comments to explain why you wrote
  the code the way you did. Show your workings.

- Don't forget to say how. For example, when writing a test, think of writing a description at the
  top to explain the goal and methodology of the test, to help your reader get up to speed, or to
  skip over sections, without forcing them to dive in.

- Comments are sentences, with a space after the slash, with a capital letter and a full stop, or a
  colon if they relate to something that follows. Comments are well-written prose describing the
  code, not just scribblings in the margin. Comments after the end of a line _can_ be phrases, with
  no punctuation.

### Cache Invalidation

- Don't duplicate variables or take aliases to them. This will reduce the probability that state
  gets out of sync.

- If you don't mean a function argument to be copied when passed by value, and if the argument type
  is more than 16 bytes, then pass the argument as `*const`. This will catch bugs where the caller
  makes an accidental copy on the stack before calling the function.

- Construct larger structs _in-place_ by passing an _out pointer_ during initialization.

  In-place initializations can assume **pointer stability** and **immovable types** while
  eliminating intermediate copy-move allocations, which can lead to undesirable stack growth.

  Keep in mind that in-place initializations are viral — if any field is initialized
  in-place, the entire container struct should be initialized in-place as well.

  **Prefer:**
  ```zig
  fn init(target: *LargeStruct) !void {
    target.* = .{
      // in-place initialization.
    };
  }

  fn main() !void {
    var target: LargeStruct = undefined;
    try target.init();
  }
  ```

  **Over:**
  ```zig
  fn init() !LargeStruct {
    return LargeStruct {
      // moving the initialized object.
    }
  }

  fn main() !void {
    var target = try LargeStruct.init();
  }
  ```

- **Shrink the scope** to minimize the number of variables at play and reduce the probability that
  the wrong variable is used.

- Calculate or check variables close to where/when they are used. **Don't introduce variables before
  they are needed.** Don't leave them around where they are not. This will reduce the probability of
  a POCPOU (place-of-check to place-of-use), a distant cousin to the infamous
  [TOCTOU](https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use). Most bugs come down to a
  semantic gap, caused by a gap in time or space, because it's harder to check code that's not
  contained along those dimensions.

- Use simpler function signatures and return types to reduce dimensionality at the call site, the
  number of branches that need to be handled at the call site, because this dimensionality can also
  be viral, propagating through the call chain. For example, as a return type, `void` trumps `bool`,
  `bool` trumps `u64`, `u64` trumps `?u64`, and `?u64` trumps `!u64`.

- Ensure that functions run to completion without suspending, so that precondition assertions are
  true throughout the lifetime of the function. These assertions are useful documentation without a
  suspend, but may be misleading otherwise.

- Be on your guard for **[buffer bleeds](https://en.wikipedia.org/wiki/Heartbleed)**. This is a
  buffer underflow, the opposite of a buffer overflow, where a buffer is not fully utilized, with
  padding not zeroed correctly. This may not only leak sensitive information, but may cause
  deterministic guarantees as required by TigerBeetle to be violated.

- Use newlines to **group resource allocation and deallocation**, i.e. before the resource
  allocation and after the corresponding `defer` statement, to make leaks easier to spot.

### Off-By-One Errors

- **The usual suspects for off-by-one errors are casual interactions between an `index`, a `count`
  or a `size`.** These are all primitive integer types, but should be seen as distinct types, with
  clear rules to cast between them. To go from an `index` to a `count` you need to add one, since
  indexes are _0-based_ but counts are _1-based_. To go from a `count` to a `size` you need to
  multiply by the unit. Again, this is why including units and qualifiers in variable names is
  important.

- Show your intent with respect to division. For example, use `@divExact()`, `@divFloor()` or
  `div_ceil()` to show the reader you've thought through all the interesting scenarios where
  rounding may be involved.

### Style By The Numbers

- Run `zig fmt`.

- Use 4 spaces of indentation, rather than 2 spaces, as that is more obvious to the eye at a
  distance.

- Hard limit all line lengths, without exception, to at most 100 columns for a good typographic
  "measure". Use it up. Never go beyond. Nothing should be hidden by a horizontal scrollbar. Let
  your editor help you by setting a column ruler. To wrap a function signature, call or data
  structure, add a trailing comma, close your eyes and let `zig fmt` do the rest.

  Similar to function length, the motivation behind the number 100 is physical: just enough
  to fit two copies of the code side-by-side on a screen.

- Add braces to the `if` statement unless it fits on a single line for consistency and defense in
  depth against "goto fail;" bugs.

### Dependencies

TigerBeetle has **a “zero dependencies” policy**, apart from the Zig toolchain. Dependencies, in
general, inevitably lead to supply chain attacks, safety and performance risk, and slow install
times. For foundational infrastructure in particular, the cost of any dependency is further
amplified throughout the rest of the stack.

### Tooling

Similarly, tools have costs. A small standardized toolbox is simpler to operate than an array of
specialized instruments each with a dedicated manual. Our primary tool is Zig. It may not be the
best for everything, but it's good enough for most things. We invest into our Zig tooling to ensure
that we can tackle new problems quickly, with a minimum of accidental complexity in our local
development environment.

> “The right tool for the job is often the tool you are already using—adding new tools has a higher
> cost than many people appreciate” — John Carmack

For example, the next time you write a script, instead of `scripts/*.sh`, write `scripts/*.zig`.

This not only makes your script cross-platform and portable, but introduces type safety and
increases the probability that running your script will succeed for everyone on the team, instead of
hitting a Bash/Shell/OS-specific issue.

Standardizing on Zig for tooling is important to ensure that we reduce dimensionality, as the team,
and therefore the range of personal tastes, grows. This may be slower for you in the short term, but
makes for more velocity for the team in the long term.

## The Last Stage

At the end of the day, keep trying things out, have fun, and remember—it's called TigerBeetle, not
only because it's fast, but because it's small!

> You don’t really suppose, do you, that all your adventures and escapes were managed by mere luck,
> just for your sole benefit? You are a very fine person, Mr. Baggins, and I am very fond of you;
> but you are only quite a little fellow in a wide world after all!”
>
> “Thank goodness!” said Bilbo laughing, and handed him the tobacco-jar.