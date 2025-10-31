---
slug: /how-to/using-auth-claims
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Using Auth Claims

SpacetimeDB allows you to easily access authentication (auth) claims embedded in OIDC-compliant JWT tokens. Auth claims are key-value pairs that provide information about the authenticated user. For example, they may contain a user's unique ID, email, or authentication provider. If you want to view these fields for yourself, you can inspect the contents of any JWT using online tools like [jwt.io](https://jwt.io/).

Within a SpacetimeDB reducer, you can access the auth claims from a client's token via the `ReducerContext`. Below are some examples of how to use these claims.

## Accessing Common Claims: Subject and Issuer

The subject (`sub`) and issuer (`iss`) are the most commonly accessed claims in a JWT. The issuer indicates which authentication provider issued the token, and the subject represents the unique identifier assigned to the user by the issuer. These are required claims, which are used to compute each user's `Identity`. Because these are so commonly used, there are helper functions to get them.

<Tabs groupId="server-language" defaultValue="typescript">
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
[Reducer(ReducerKind.ClientConnected)]
public static void ClientConnected(ReducerContext ctx)
{
    var claims = ctx.SenderAuth.Jwt ?? throw new Exception("Client connected without JWT");
    Log.Info($"Client connected with csub: {claims.Subject}, and iss: {claims.Issuer}");
}
```

</TabItem>
<TabItem value="typescript" label="TS">

```typescript
import { SenderError } from "spacetimedb/server";

spacetimedb.clientConnected((ctx) => {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError("Unauthorized: JWT is required to connect");
  }
  console.info(`Client connected with sub: ${jwt.subject}, iss: ${jwt.issuer}`);
});
```

</TabItem>
</Tabs>

## Example: Restricting auth providers

Since users can use any valid token to connect to SpacetimeDB, their token may originate from any authentication provider. For example, they could send an OIDC compliant token from GitHub even though you only want to accept tokens from Google. It often makes sense to restrict access to your module to users issued by your own issuer or specific issuers. It is best practice to check at least the issuer when clients connect, so you can ensure that your data can only be accessed by users of your application.

Additionally, it's imperative that you check the `aud` claim to ensure the issuer intended your application to receive the token. This ensures that applications which send you the token, cannot repurpose tokens issued for use with another application.

For example, we can restrict access to clients with SpacetimeAuth credentials.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
// Set this to your the OIDC client (or set of clients) set up for your
// SpacetimeAuth project.
const OIDC_CLIENT_ID: &str = "client_XXXXXXXXXXXXXXXXXXXXXX";

#[reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let jwt = ctx.sender_auth().jwt().ok_or("Authentication required".to_string())?;
    if jwt.issuer() != "https://auth.spacetimedb.com/oidc" {
        return Err("Invalid issuer".to_string());
    }

    if !jwt.audience().iter().any(|a| a == OIDC_CLIENT_ID) {
        return Err("Invalid audience".to_string());
    }
    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
// The oidc client ids configured for SpacetimeAuth.
public static readonly List<string> OIDC_CLIENT_IDS = new()
{
    "client_XXXXXXXXXXXXXXXXXXXXXX",
};
public void Connect(ReducerContext ctx)
{
    var claims = ctx.SenderAuth.Jwt ?? throw new Exception("Client connected without JWT");
    if (claims.Issuer != "https://auth.spacetimedb.com/oidc")
    {
        throw new Exception("Unauthorized: invalid issuer");
    }
    if (!OIDC_CLIENT_IDS.Any(s => claims.Audience.Contains(s)))
    {
        throw new Exception("Unauthorized: invalid audience");
    }
}
```

</TabItem>
<TabItem value="typescript" label="TS">

```typescript
const OIDC_CLIENT_IDS = ["client_XXXXXXXXXXXXXXXXXXXXXX"];
spacetimedb.clientConnected((ctx) => {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError("Unauthorized: JWT is required to connect");
  }
  if (jwt.issuer != "https://auth.spacetimedb.com/oidc") {
    throw new SenderError(`Unauthorized: Invalid issuer ${jwt.issuer}`);
  }
  if (!jwt.audience.some((aud) => OIDC_CLIENT_IDS.includes(aud))) {
    throw new SenderError(`Unauthorized: Invalid audience ${jwt.audience}`);
  }
});
```

</TabItem>
</Tabs>

## Accessing custom claims

If you want to access additional claims that aren't available via helper functions, you can parse the full JWT payload. This is useful for handling custom or application-specific claims.

As an example, let's say that your tokens have a "roles" claim, which is a list of priviledges. If you want to make sure that only users with the `admin` role are able to call a certain reducer, you could do the following:

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

Update your `Cargo.toml` like so to add `serde` and `serde_json` for parsing json:

```toml
[dependencies]
...

serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.143"
```

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomClaims {
    pub roles: Vec<String>,
}

/// Returns Ok(()) if the sender has admin access, Err otherwise.
fn ensure_admin_access(sender_auth: &spacetimedb::AuthCtx) -> Result<(), String> {
    if sender_auth.is_internal() {
        // This is a scheduled reducer, so it should already be trusted.
        return Ok(());
    }
    let jwt = sender_auth.jwt().ok_or("Authentication required".to_string())?;
    let claims: CustomClaims = serde_json::from_slice(jwt.raw_payload().as_bytes()).map_err(|e| format!("Client connected with invalid JWT: {}", e).to_string())?;

    if claims.roles.iter().any(|r| r == "admin") {
        return Ok(());
    }
    Err("Admin role required".to_string())
}

#[spacetimedb::reducer]
pub fn admin_only_reducer(ctx: &ReducerContext) -> Result<(), String> {
    ensure_admin_access(&ctx.sender_auth())?;
    // Now we can safely perform admin-only actions.
    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```cs
// Throw if the sender does not have admin access.
private static void EnsureAdminAccess(ReducerContext ctx)
{
    var auth = ctx.SenderAuth;
    if (auth.IsInternal)
    {
        return;
    }

    var claims = auth.Jwt ?? throw new Exception("Missing JWT claims");

    using var jwtPayload = JsonDocument.Parse(claims.RawPayload);
    var root = jwtPayload.RootElement;
    bool hasAdmin =
        root.TryGetProperty("roles", out var rolesProp) &&
        rolesProp.ValueKind == JsonValueKind.Array &&
        rolesProp.EnumerateArray().Any(e =>
            e.ValueKind == JsonValueKind.String && e.GetString() == "admin"
        );

    if (!hasAdmin)
    {
        throw new Exception("Unauthorized: admin role required");
    }
}

[Reducer]
public static void AdminOnlyReducer(ReducerContext ctx)
{
    EnsureAdminAccess(ctx);
    // We can now be sure that the caller is an admin.
}
```

</TabItem>
<TabItem value="typescript" label="TS">

```typescript

// Return an error to the client if they don't have admin rights.
function ensureAdminAccess(ctx: ReducerCtx<any>) {
  const auth = ctx.senderAuth;
  if (auth.isInternal) {
    return;
  }
  const jwt = auth.jwt;
  if (jwt == null) {
    throw new SenderError("Unauthorized: JWT is required");
  }
  const roles = jwt.fullPayload["roles"];
  if (!Array.isArray(roles) || !roles.includes("admin")) {
    throw new SenderError("Unauthorized: Admin role is required");
  }
}

spacetimedb.reducer("adminonly", (ctx) => {
  ensureAdminAccess(ctx);
});
```

</TabItem>
</Tabs>


## Summary and Best Practices

- Always validate the presence and contents of JWT claims before trusting them in your application logic.
- For custom application logic, deserialize the JWT payload to access additional claims which are not parsed by default.
- Restrict accepted issuers where appropriate to enforce security policies.

For more information, refer to the [SpacetimeDB documentation](https://spacetimedb.com/docs/) or reach out to the SpacetimeDB community for help.