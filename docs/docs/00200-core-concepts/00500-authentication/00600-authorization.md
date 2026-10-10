# Authorization

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Authentication tells SpacetimeDB who is calling. Authorization decides what they
are allowed to do. This page explains what your module knows about each
invocation of a reducer, procedure, or HTTP handler, what each of those facts
means, and how to use them to decide what a caller may do.

## Three layers

Every invocation passes through three layers, in order:

1. **Authentication.** SpacetimeDB establishes who the sender is and which
   token, if any, they presented. Your module doesn't do this itself; it
   receives the results.
2. **Host authorization.** SpacetimeDB checks the function's
   [visibility](#function-visibility) before running it. A caller who isn't
   allowed to call a function never reaches your code.
3. **Module authorization.** Your code decides what the sender may do, using
   the sender, whether the invocation is [internal](#internal-invocations), and
   the claims in the sender's token. This includes rejecting a connection in
   `client_connected`.

Each layer relies on the answers of the layers before it instead of working
them out again. In particular, your module shouldn't try to infer who is calling
from indirect signals, such as whether a connection ID or a token happens to be
present.

## What you know about an invocation

- **The sender** is the `Identity` responsible for the invocation. When a
  client calls a function, the sender is that client. When SpacetimeDB runs a
  function because something happened, the sender is whoever caused it: the
  database itself for a scheduled function, the owner for `init`, and the
  connecting client for `client_connected`.
- **The connection ID** tells you which connection the invocation came from.
  It's useful for tracking sessions, but it never tells you who the sender is,
  and a missing connection ID doesn't tell you anything about the sender either.
- **The JWT** holds the claims of the token the sender presented, if they
  presented one. See [Using Auth Claims](./00500-usage.md).
- **Whether the invocation is internal** tells you whether the sender is the
  database itself.
- **The function's visibility** determines who may call the function at all.

## Who the sender is

| Invocation | Sender | Internal | JWT |
| ---------- | ------ | -------- | --- |
| A client calls a reducer or procedure | the client | no | the client's token |
| A scheduled reducer or procedure runs | the database | yes | none |
| `client_connected` or `client_disconnected` runs | the client | no | the client's token |
| `init` runs, including when the database is cleared | the database's owner | no | none |
| A procedure opens a transaction | the procedure's sender | same as the procedure | same as the procedure |
| An HTTP handler opens a transaction | the zero identity | no | none |

Scheduling doesn't carry the scheduling sender forward. A scheduled function's
sender is always the database, no matter who inserted the schedule row. If a
client can influence what goes into a schedule row, for example through a
public reducer that inserts one using the client's arguments, the scheduled
function should validate the row the same way a public reducer validates its
arguments.

SpacetimeDB doesn't authenticate the `Authorization` header of requests to
[HTTP handlers](../00200-functions/00600-HTTP-handlers.md). A handler that needs
to know who is calling must validate the request's credentials itself.

## Internal invocations

An invocation is internal exactly when its sender is the database itself.
Checking whether an invocation is internal is equivalent to comparing the sender
with the database's identity, and it's how you tell the database acting on its
own behalf apart from everyone else, including the database's owner. Today, the
database's internal invocations are its scheduled reducers and procedures.
Internal invocations never have a JWT.

Most modules don't need this check, because [visibility](#function-visibility)
already keeps clients away from scheduled functions. Use it when a function
must only ever run as the database itself, for example a scheduled reducer that
the database's owner must not be able to run by hand.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
if (!ctx.senderAuth.isInternal) {
  throw new SenderError('Only the database itself may run this');
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
if (!ctx.SenderAuth.IsInternal)
{
    throw new Exception("Only the database itself may run this");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
if !ctx.sender_auth().is_internal() {
    return Err("Only the database itself may run this".into());
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
if (!ctx.sender_auth().is_internal()) {
    return Err("Only the database itself may run this");
}
```

</TabItem>
</Tabs>

## Function visibility

Visibility is a property of a function that SpacetimeDB checks before the
function runs:

- **Public** functions can be called by any client. Reducers and procedures are
  public unless they fall into one of the categories below.
- **Private** functions can't be called by clients. Only the database's owner
  can call them directly, and the database runs them through its scheduler.
  Scheduled reducers and procedures are private. If anyone else calls a private
  function, SpacetimeDB responds as if it didn't exist.
- **Lifecycle reducers**, which are `init`, `client_connected`, and
  `client_disconnected`, can't be called by anyone. SpacetimeDB runs them when
  their event occurs.

## Best practices

- Prefer visibility to checks in function bodies. A scheduled function is
  already private, so you don't need to check its sender to keep clients out.
- Check authorization where a caller enters your module, at the top of each
  public reducer or procedure, rather than inside helpers that other functions
  also call. For example, a helper that lets internal invocations through will
  reject `init`, because the sender of `init` is the owner.
- Only check whether an invocation is internal when a function must exclude the
  database's owner as well as clients.
- Check whether an invocation is internal rather than comparing the sender with
  the database's identity yourself. The two are equivalent, but the first says
  what you mean.
- Don't treat a missing connection ID or a missing JWT as a sign of anything
  about the sender. `init` and HTTP handlers have neither, and neither is
  internal.
- In a scheduled function, treat the schedule row as data written by whoever
  caused the write.
- To restrict functions to your own administrators, record the owner in `init`,
  as described in [Lifecycle Reducers](../00200-functions/00300-reducers/00500-lifecycle.md#init-reducer),
  or check custom claims in the sender's token, as described in
  [Using Auth Claims](./00500-usage.md#accessing-custom-claims).
