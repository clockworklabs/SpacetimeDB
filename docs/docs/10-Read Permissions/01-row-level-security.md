---
title: Row Level Security
slug: /rls
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Row Level Security (RLS)

Row Level Security (RLS) allows module authors to restrict which rows of a public table each client can access.
These access rules are expressed in SQL and evaluated automatically for queries and subscriptions.

## Enabling RLS

RLS is currently **experimental** and must be explicitly enabled in your module.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
To enable RLS, activate the `unstable` feature in your project's `Cargo.toml`:

```toml
spacetimedb = { version = "...", features = ["unstable"] }
```

</TabItem>
<TabItem value="csharp" label="C#">
To enable RLS, include the following preprocessor directive at the top of your module files:

```cs
#pragma warning disable STDB_UNSTABLE
```

</TabItem>
</Tabs>

## How It Works

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
RLS rules are expressed in SQL and declared as constants of type `Filter`.

```rust
use spacetimedb::{client_visibility_filter, Filter};

/// A client can only see their account
#[client_visibility_filter]
const ACCOUNT_FILTER: Filter = Filter::Sql(
    "SELECT * FROM account WHERE account.identity = :sender"
);
```

</TabItem>
<TabItem value="csharp" label="C#">
RLS rules are expressed in SQL and declared as public static readonly fields of type `Filter`.

```cs
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

public partial class Module
{
    /// <summary>
    /// A client can only see their account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT * FROM account WHERE account.identity = :sender"
    );
}
```

</TabItem>
</Tabs>

A module will fail to publish if any of its RLS rules are invalid or malformed.

### `:sender`

You can use the special `:sender` parameter in your rules for user specific access control.
This parameter is automatically bound to the requesting client's [Identity].

Note that module owners have unrestricted access to all tables regardless of RLS.

[Identity]: /#identity

### Semantic Constraints

RLS rules are similar to subscriptions in that logically they act as filters on a particular table.
Also like subscriptions, arbitrary column projections are **not** allowed.
Joins **are** allowed, but each rule must return rows from one and only one table.

### Multiple Rules Per Table

Multiple rules may be declared for the same table and will be evaluated as a logical `OR`.
This means clients will be able to see to any row that matches at least one of the rules.

#### Example

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{client_visibility_filter, Filter};

/// A client can only see their account
#[client_visibility_filter]
const ACCOUNT_FILTER: Filter = Filter::Sql(
    "SELECT * FROM account WHERE account.identity = :sender"
);

/// An admin can see all accounts
#[client_visibility_filter]
const ACCOUNT_FILTER_FOR_ADMINS: Filter = Filter::Sql(
    "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE

public partial class Module
{
    /// <summary>
    /// A client can only see their account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT * FROM account WHERE account.identity = :sender"
    );

    /// <summary>
    /// An admin can see all accounts.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER_FOR_ADMINS = new Filter.Sql(
        "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
    );
}
```

</TabItem>
</Tabs>

### Recursive Application

RLS rules can reference other tables with RLS rules, and they will be applied recursively.
This ensures that data is never leaked through indirect access patterns.

#### Example

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{client_visibility_filter, Filter};

/// A client can only see their account
#[client_visibility_filter]
const ACCOUNT_FILTER: Filter = Filter::Sql(
    "SELECT * FROM account WHERE account.identity = :sender"
);

/// An admin can see all accounts
#[client_visibility_filter]
const ACCOUNT_FILTER_FOR_ADMINS: Filter = Filter::Sql(
    "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
);

/// Explicitly filtering by client identity in this rule is not necessary,
/// since the above RLS rules on `account` will be applied automatically.
/// Hence a client can only see their player, but an admin can see all players.
#[client_visibility_filter]
const PLAYER_FILTER: Filter = Filter::Sql(
    "SELECT p.* FROM account a JOIN player p ON a.id = p.id"
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
using SpacetimeDB;

public partial class Module
{
    /// <summary>
    /// A client can only see their account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT * FROM account WHERE account.identity = :sender"
    );

    /// <summary>
    /// An admin can see all accounts.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER_FOR_ADMINS = new Filter.Sql(
        "SELECT account.* FROM account JOIN admin WHERE admin.identity = :sender"
    );

    /// <summary>
    /// Explicitly filtering by client identity in this rule is not necessary,
    /// since the above RLS rules on `account` will be applied automatically.
    /// Hence a client can only see their player, but an admin can see all players.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter PLAYER_FILTER = new Filter.Sql(
        "SELECT p.* FROM account a JOIN player p ON a.id = p.id"
    );
}
```

</TabItem>
</Tabs>

And while self-joins are allowed, in general RLS rules cannot be self-referential,
as this would result in infinite recursion.

#### Example: Self-Join

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{client_visibility_filter, Filter};

/// A client can only see players on their same level
#[client_visibility_filter]
const PLAYER_FILTER: Filter = Filter::Sql("
    SELECT q.*
    FROM account a
    JOIN player p ON a.id = p.id
    JOIN player q on p.level = q.level
    WHERE a.identity = :sender
");
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
using SpacetimeDB;

public partial class Module
{
    /// <summary>
    /// A client can only see players on their same level.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter PLAYER_FILTER = new Filter.Sql(@"
        SELECT q.*
        FROM account a
        JOIN player p ON a.id = p.id
        JOIN player q on p.level = q.level
        WHERE a.identity = :sender
    ");
}
```

</TabItem>
</Tabs>

#### Example: Recursive Rules

This module will fail to publish because each rule depends on the other one.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{client_visibility_filter, Filter};

/// An account must have a corresponding player
#[client_visibility_filter]
const ACCOUNT_FILTER: Filter = Filter::Sql(
    "SELECT a.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
);

/// A player must have a corresponding account
#[client_visibility_filter]
const PLAYER_FILTER: Filter = Filter::Sql(
    "SELECT p.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
using SpacetimeDB;

public partial class Module
{
    /// <summary>
    /// An account must have a corresponding player.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT a.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
    );

    /// <summary>
    /// A player must have a corresponding account.
    /// </summary>
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ACCOUNT_FILTER = new Filter.Sql(
        "SELECT p.* FROM account a JOIN player p ON a.id = p.id WHERE a.identity = :sender"
    );
}
```

</TabItem>
</Tabs>

## Usage in Subscriptions

RLS rules automatically apply to subscriptions so that if a client subscribes to a table with RLS filters,
the subscription will only return rows that the client is allowed to see.

While the contraints and limitations outlined in the [reference docs] do not apply to RLS rules,
they do apply to the subscriptions that use them.
For example, it is valid for an RLS rule to have more joins than are supported by subscriptions.
However a client will not be able to subscribe to the table for which that rule is defined.

[reference docs]: /sql#subscriptions

## Best Practices

1. Use `:sender` for client specific filtering.
2. Follow the [SQL best practices] for optimizing your RLS rules.

[SQL best practices]: /sql#best-practices-for-performance-and-scalability
