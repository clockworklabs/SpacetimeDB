---
slug: /how-to/using-auth-claims
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Using Auth Claims

SpacetimeDB allows you to easily access authentication (auth) claims embedded in OIDC-compliant JWT tokens. Auth claims are key-value pairs that provide information about the authenticated user. For example, they may contain a user's unique ID, email, or authentication provider. If you want to view these fields for yourself, you can inspect the contents of any JWT using online tools like [jwt.io](https://jwt.io/).

Within a SpacetimeDB reducer, you can access the auth claims from a client's token via the `ReducerContext`. Below are some examples of how to use these claims.

## Accessing Common Claims: Subject and Issuer

The subject (`sub`) and issuer (`iss`) are the most commonly accessed claims in a JWT. The subject usually represents the user's unique identifier, while the issuer indicates which authentication provider issued the token. These are required claims, which are used to compute each user's `Identity`. Because these are so commonly used, there are helper functions to get them.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let auth_ctx = ctx.sender_auth();
    let (subject, issuer) = match auth_ctx.jwt() {
        Some(claims) => (claims.subject().to_string(), claims.issuer().to_string()),
        None => {
            return Err("Client connected without JWT".to_string());
        }
    };
    log::info!("sub: {}, iss: {}", subject, issuer);
    Ok(())
}
```

*Example output when using a Google-issued token:*
```
INFO: src\lib.rs:64: sub: 321321321321321, iss: https://accounts.google.com
```
</TabItem>
<TabItem value="csharp" label="C#">

```cs
// ************************TODO: update + test this after Jeff implements this in C# ************************
[Reducer(ReducerKind.ClientConnected)]
public void Connect(ReducerContext ctx) {
    var auth_ctx = ctx.SenderAuth();
    var (subject, issuer) = auth_ctx.Jwt() switch {
        Some(var claims) => (claims.Subject, claims.Issuer),
        None => throw new Exception("Client connected without JWT"),
    };
    log.Info($"sub: {subject}, iss: {issuer}");
}
```

*Example output when using a Google-issued token:*
```
INFO: src\Lib.cs:64: sub: 321321321321321, iss: https://accounts.google.com
```

</TabItem>
<TabItem value="typescript" label="TS">
dummy
</TabItem>
</Tabs>

## Accessing custom claims

If you want to access additional claims that aren't available via helper functions, you can parse the full JWT payload. This is useful for handling custom or application-specific claims.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```toml
[dependencies]
...

log = "0.4"
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.143"
```

```rust
#[reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let auth_ctx = ctx.sender_auth();
    let (subject, issuer) = match auth_ctx.jwt() {
        Some(claims) => (claims.subject().to_string(), claims.issuer().to_string()),
        None => {
            return Err("Client connected without JWT".to_string());
        }
    };
    log::info!("sub: {}, iss: {}", subject, issuer);
    Ok(())
}
```

*Example output when using a Google-issued token:*
```
INFO: src\lib.rs:64: sub: 321321321321321, iss: https://accounts.google.com
```
</TabItem>
<TabItem value="csharp" label="C#">

```cs
// ************************TODO: update + test this after Jeff implements this in C# ************************
[Reducer(ReducerKind.ClientConnected)]
public void Connect(ReducerContext ctx) {
    var auth_ctx = ctx.SenderAuth();
    var (subject, issuer) = auth_ctx.Jwt() switch {
        Some(var claims) => (claims.Subject, claims.Issuer),
        None => throw new Exception("Client connected without JWT"),
    };
    log.Info($"sub: {subject}, iss: {issuer}");
}
```

*Example output when using a Google-issued token:*
```
INFO: src\Lib.cs:64: sub: 321321321321321, iss: https://accounts.google.com
```

</TabItem>
<TabItem value="typescript" label="TS">
dummy
</TabItem>
</Tabs>

## Example: Restricting auth providers

Since users can use any valid token to connect to SpacetimeDB, their token may originate from any authentication provider. For example, they could send an OIDC compliant token from Github even though you only want to accept tokens from Google. It is best practice to check at least the issuer and audience of tokens when clients connect, so you can ensure that your data can only be accessed by users of your application.

For example, we can restrict access to clients with SpacetimeAuth credentials.