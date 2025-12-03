---
title: Maincloud
slug: /deploying/maincloud
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Maincloud is a managed cloud service that provides developers an easy way to deploy their SpacetimeDB apps to the cloud.

## Deploy via CLI

1. Install the SpacetimeDB CLI for your platform: [Install SpacetimeDB](https://spacetimedb.com/install)
1. Create your module (see [Getting Started](/getting-started))
1. Publish to Maincloud:

```bash
spacetime publish -s maincloud my-cool-module
```

## Connecting your Identity to the Web Dashboard

By logging in your CLI via spacetimedb.com, you can view your published modules on the web dashboard.

If you did not log in with spacetimedb.com when publishing your module, you can log in by running:

```bash
spacetime logout
spacetime login
```

1. Open the SpacetimeDB website and log in using your GitHub login.
1. You should now be able to see your published modules [https://spacetimedb.com/profile](https://spacetimedb.com/profile), or you can navigate to your database directly at [https://spacetimedb.com/my-cool-module](https://spacetimedb.com/my-cool-module).

---

With SpacetimeDB Maincloud, you benefit from automatic scaling, robust security, and the convenience of not having to manage the hosting environment.

# Connect from Client SDKs

To connect to your deployed module in your client code, use the host url of `https://maincloud.spacetimedb.com`:

<Tabs groupId="syntax" queryString>
<TabItem value="typescript" label="TypeScript">

```ts
DbConnection.builder().withUri('https://maincloud.spacetimedb.com');
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
DbConnection.Builder()
    .WithUri("https://maincloud.spacetimedb.com")
```
</TabItem>

</Tabs>