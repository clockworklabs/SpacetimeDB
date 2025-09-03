### Documentation TODO

- Update C# examples when it is implemented
- Update the rust examples one last time when auth claims are merged

I asked Jeff:

Can auth claims change after a user has already authenticated? Like if I connect a client which authenticates via SpacetimeAuth but then after they connect they change their phone number or address or something else that would be in their auth claims - when will the module see that change?

^ Update documentation for the response to this

VVV Documentation starts below VVV

# Accessing Auth Claims in Modules

SpacetimeDB allows you to easily access authentication (auth) claims embedded in OIDC-compliant JWT tokens. Auth claims are key-value pairs that provide information about the authenticated user. For example, they may contain a user's unique ID, email, or authentication provider. If you want to view these fields for yourself, you can inspect the contents of any JWT using online tools like [jwt.io](https://jwt.io/).

In a SpacetimeDB module, auth claims from a client's token are accessible via the `ReducerContext` which is passed to all reducers. The following examples show how to access and use these claims in your module code.

## Accessing Common Claims: Subject and Issuer

The subject (`sub`) and issuer (`iss`) are the most commonly accessed claims in a JWT. The subject usually represents the user's unique identifier, while the issuer indicates which authentication provider issued the token.

Below are examples of how to access these claims in both Rust and C# modules:

:::server-rust

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
:::
:::server-csharp

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
:::

## Accessing Custom Claims

If you want to access additional claims that are not parsed by default by SpacetimeDB, you can parse the raw JWT payload yourself. This is useful for handling custom or application-specific claims.

Below is a Rust example for extracting a custom claim (e.g., email):

:::server-rust

If you'd like to compile this example, please start with modifying your Cargo.toml to include the following dependencies:

```toml
[dependencies]
...

log = "0.4"
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.143"
```

```rust
use spacetimedb::{reducer, table, ReducerContext, Table};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomClaims {
    pub iss: String,
    pub sub: String,
    pub iat: u64,
    pub email: String,
}

#[table(name = user)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub email: String,
}

#[reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let auth_ctx = ctx.sender_auth();
    let payload = auth_ctx.jwt().unwrap().raw_payload();
    let claims: CustomClaims = serde_json::from_slice(payload.as_bytes()).map_err(|e| format!("Client connected with invalid JWT: {}", e).to_string())?;

    // In this example, we'll identify users based on their email
    if ctx.db.user().email().find(&claims.email).is_none() {
        ctx.db.user().insert(User {
            id: 0,
            email: claims.email.clone(),
        });
        log::info!("Created new user with email: {}.", claims.email);
    } else {
        log::info!("User with email {} has returned.", claims.email);
    }

    Ok(())
}
```
:::
:::server-csharp
```cs
// ************************TODO: update + test this after Jeff implements this in C# ************************
[Reducer(ReducerKind.ClientConnected)]
public void Connect(ReducerContext ctx) {
    var auth_ctx = ctx.SenderAuth();
    var jwt = auth_ctx.Jwt().Value;
    var custom_claim = jwt.Claims["custom_claim"].Value;
}
```
:::


## Example: Restricting Accepted Issuers

Since users can use any valid token to connect to SpacetimeDB, their token may originate from any authentication provider. For example, they could send an OIDC compliant token from Github even though you only want to accept tokens from Google. If you want to only allow users to authenticate using a specific provider (e.g., Google), you can check the issuer claim `iss` as shown below:

:::server-rust

```rust
#[reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let auth_ctx = ctx.sender_auth();
    if auth_ctx.jwt().is_none() {
        return Err("Client connected without JWT".to_string());
    }

    let jwt = auth_ctx.jwt().unwrap();
    // Example: We only accept google auth
    if jwt.issuer() != "https://accounts.google.com" {
        return Err(format!("Client connected with a JWT from an unaccepted issuer: {}", jwt.issuer()));
    }

    Ok(())
}
```
:::
:::server-csharp
```cs
[Reducer(ReducerKind.ClientConnected)]
public void Connect(ReducerContext ctx) {
    var authContext = ctx.SenderAuth();
    if (authContext.Jwt() == null) {
        throw new Exception("Client connected without JWT");
    }

    // Example: We only accept google auth
    var jwt = authContext.Jwt();
    if (jwt.Issuer != "https://accounts.google.com") {
        throw new Exception($"Client connected with a JWT from an unaccepted issuer: {jwt.Issuer}");
    }
}
```
:::

> **Important:** If you return an error from the connect reducer, the client will be disconnected immediately.

## Example: automatically give users a role based on their auth claims

When users first connect to your SpacetimeDB module you may want to automatically give them some role. For example, at clockwork we might want to give users with a valid "clockworklabs.io" email an admin role. We can do this by checking the email claim in the connect reducer and giving them the admin role if they have a valid email.

:::server-rust

```rust
use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomClaims {
    pub iss: String,
    pub sub: String,
    pub iat: u64,
    pub email: String,
}

#[table(name = user)]
pub struct User {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[unique]
    pub email: String,
}

#[derive(SpacetimeType)]
pub enum Role {
    User,
    Admin,
}

#[table(name = user_role)]
pub struct UserRole {
    #[primary_key]
    pub id: u64,
    pub role: Role,
}


#[reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    let auth_ctx = ctx.sender_auth();
    let payload = auth_ctx.jwt().unwrap().raw_payload();
    let claims: CustomClaims = serde_json::from_slice(payload.as_bytes()).map_err(|e| format!("Client connected with invalid JWT: {}", e).to_string())?;

    // In this example you would really want to check the issuer of the JWT to ensure it is from a trusted provider
    if auth_ctx.jwt().unwrap().issuer() != "https://accounts.google.com" {
        return Err("Client connected with a JWT from an unaccepted issuer".to_string());
    }

    if ctx.db.user().email().find(&claims.email).is_none() {
        let user = ctx.db.user().insert(User {
            id: 0,
            email: claims.email.clone(),
        });

        if claims.email.ends_with("clockworklabs.io") {
            ctx.db.user_role().insert(UserRole {
                id: user.id,
                role: Role::Admin,
            });
            log::info!("Created new admin user with email: {}.", claims.email);
        } else {
            log::info!("Created new normal user with email: {}.", claims.email);
        }

    } else {
        log::info!("User with email {} has returned.", claims.email);
    }

    Ok(())
}
```
:::
:::server-csharp
```cs
// ************************TODO: update + test this after Jeff implements this in C# ************************
[Reducer(ReducerKind.ClientConnected)]
public void Connect(ReducerContext ctx) {
    var authContext = ctx.SenderAuth();
    if (authContext.Jwt() == null) {
        throw new Exception("Client connected without JWT");
    }

    var jwt = authContext.Jwt();
    // In this example you would really want to check the issuer of the JWT to ensure it is from a trusted provider
    if (jwt.Issuer != "https://accounts.google.com") {
        throw new Exception("Client connected with a JWT from an unaccepted issuer");
    }

    var email = jwt.Claims.FirstOrDefault(c => c.Type == "email")?.Value;
    if (ctx.Db.User().Email().Find(email) == null) {
        var user = ctx.Db.User().Insert(new User {
            Id = 0,
            Email = email
        });

        if (email.EndsWith("@clockworklabs.io")) {
            ctx.Db.UserRole().Insert(new UserRole {
                Id = user.Id,
                Role = Role.Admin
            });
            Serilog.Log.Information("Created new admin user with email: {Email}.", email);
        } else {
            Serilog.Log.Information("Created new normal user with email: {Email}.", email);
        }

    } else {
        Serilog.Log.Information("User with email {Email} has returned.", email);
    }
}
```
:::

---

## Summary & Best Practices

- Always validate the presence and contents of JWT claims before trusting them in your application logic.
- For custom application logic, deserialize the JWT payload to access additional claims which are not parsed by default.
- Restrict accepted issuers where appropriate to enforce security policies.

For more information, refer to the [SpacetimeDB documentation](https://spacetimedb.com/docs/) or reach out to the SpacetimeDB community for help.