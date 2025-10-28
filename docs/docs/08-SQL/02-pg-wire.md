---
title: PostgreSQL Wire Protocol (PGWire)
slug: /docs/sql/pg-wire
---

# PostgreSQL Wire Protocol (PGWire) Compatibility

_SpacetimeDB_ supports the [PostgreSQL wire protocol (](https://www.postgresql.org/docs/current/protocol.html)[
_PGWire_](https://www.postgresql.org/docs/current/protocol.html)[)](https://www.postgresql.org/docs/current/protocol.html),
enabling compatibility with PostgreSQL clients and tools.

The PostgreSQL wire protocol is a network protocol used by PostgreSQL clients to communicate with compatible servers. It
defines how messages are formatted and exchanged between client and server. The protocol is agnostic to the query
dialect,
meaning it can be used with different SQL engines and feature sets, in concrete, _SpacetimeDB_.

This allows users to leverage the existing PostgreSQL ecosystem, including drivers, ORMs, IDEs, CLI tools, and GUI
clients that support PostgreSQL.

When using _PGWire_ with _SpacetimeDB_, consider the following:

## Feature Support

SpacetimeDB is progressively adding PostgreSQL client compatibility. Some features are unsupported, partially
implemented, or behave differently:

- **Protocol Version**: Only _PGWire_ protocol _version 3.0_ is supported, and only the _Simple Query Protocol_, and
  without parameterized queries.
- **SQL Features**: Only the subset of SQL features documented in the [SQL documentation](/sql) are
  supported. Subscription queries do not update in real time.
- **Authentication**: SpacetimeDB does not implement database users or roles. The connection string
  `user_name@database_name` ignores `user_name`; only `database_name` is used. Authentication is based on the _auth
  token_
  provided via the `password` field.
- **SSL/TLS**: SSL is supported only for `SpacetimeDB Cloud` deployments (without mutual TLS). Other deployments (such
  as `SpacetimeDB Standalone`) do not support SSL/TLS connections.
- **System Tables and Views**: SpacetimeDB provides its own system tables (e.g., `SELECT * FROM st_table`) for
  introspection. These are not PostgreSQL-compatible, so tools relying on PostgreSQL system catalogs will not work.
- **Port and Host**:
  - In `SpacetimeDB Standalone` deployments, specify the port with `spacetime start --pg-port <port>`. Without this
    flag, connections using the PostgreSQL protocol are not enabled.
  - In `SpacetimeDB Cloud` deployments, the port is always `5432`.
- **Transactions**: User-defined transactions (`BEGIN TRANSACTION`, `COMMIT`, etc.) are not supported. Each SQL
  statement executes in its own transaction context. Client libraries should disable automatic transaction handling.
- **Special Data Types**: Some SpacetimeDB data types map to PostgreSQL types as:
  - Simple enums are displayed as `Enum`.
  - Algebraic Data Types (ADTs) & records are displayed as `JSON`.
  - `Duration` is displayed as `Interval`.
  - `Identity`, `ConnectionId`, `U8`, `[U8]`, `Bytes` & `Hex` is displayed as `Bytea`.

## Connection Parameters

To connect to SpacetimeDB using a PostgreSQL client, use the following parameters:

- **Host**:
  - `localhost` for `SpacetimeDB Standalone` deployments
  - `maincloud.spacetimedb.com` for `SpacetimeDB Cloud` deployments
- **Port**:
  - `5432` for `SpacetimeDB Cloud`
  - The value passed with `--pg-port` for `SpacetimeDB Standalone`
- **Database**: The target SpacetimeDB database
- **User**: Any string (ignored by SpacetimeDB)
- **Password**: The `auth token`
- **SSL Mode**: `require` (only for `SpacetimeDB Cloud`)

### Auth Token

:::warning

The `auth token` is sensitive. Do not expose it in logs, version control, or insecure locations.

:::

SpacetimeDB uses the `password` field to pass the `auth token`. Obtain the token with:

```bash
spacetime login show --token
```

To export the token to `PGPASSWORD`:

_For bash_:

```bash
export PGPASSWORD="$(spacetime login show --token | sed -n 's/^Your auth token.*is //p')"
```

_For PowerShell_:

```powershell
$env:PGPASSWORD = (spacetime login show --token | Select-String 'Your auth token.*is (.*)' | % { $_.Matches[0].Groups[1].Value })
```

### Enabling _PGWire_ in SpacetimeDB Standalone

_PGWire_ is disabled by default when starting a `SpacetimeDB Standalone` server.

To enable it, start the server with the `--pg-port` option:

```bash
spacetime start --pg-port 5432 [ARGS]
```

## Examples

In the following example, we assume you are using the `quickstart-chat` database created in
the [Rust Module Quickstart](/modules/rust/quickstart) or [C# Module Quickstart](/modules/c-sharp/quickstart),
and have set the `auth token` as shown above.

### Using `psql`

SpacetimeDB Standalone deployment:

```bash
psql "host=localhost port=5432 user=any dbname=quickstart-chat"
```

SpacetimeDB Cloud deployment:

```bash
psql "host=maincloud.spacetimedb.com port=5432 user=any dbname=quickstart-chat sslmode=require"
```

:::note

Introspection commands such as `\dt` will not work, as SpacetimeDB does not support PostgreSQL schemas.

:::

Now for example:

```psql
quickstart=> select * from message;
                               sender                               |               sent               | text
--------------------------------------------------------------------+----------------------------------+-------
 \xc200da2d6ddb6c0beef0bbaafacffe5f0649c86b8d19411e3219066a6d0e5123 | 2025-09-29T22:29:14.271647+00:00 | hello
(1 row)

quickstart=> update message set text = 'world';
updated: 1, server: 1.72ms

quickstart=> select text from message;
 text
-------
 world
(1 row)
```

### Using Python (`psycopg2`)

```python
import psycopg2
import os

conn = psycopg2.connect(
    host="localhost",  # or "maincloud.spacetimedb.com" for SpacetimeDB Cloud
    port=5432,
    dbname="quickstart-chat",
    user="any",
    password=os.getenv("PGPASSWORD"),
    sslmode="disable"  # use "require" for SpacetimeDB Cloud
)
conn.set_session(autocommit=True)  # disable transactions

print("Running query:")
with conn.cursor() as cur:
    cur.execute("SELECT * FROM message;")
    for row in cur.fetchall():
        print(row)

conn.close()
print("Done.")
```

### Using Rust (`tokio-postgres` + `rustls`)

We use the `tokio-postgres-rustls` because is stricter, so we can show how disables certificate verification.

```toml
# Cargo.toml
[dependencies]
anyhow = "1.0.71"
tokio-postgres = "0.7.14"
tokio-postgres-rustls = "0.13.0"
tokio = { version = "1.47.1", features = ["full"] }
rustls = "0.23.32"
```

```rust
// main.rs
use std::env;
use std::sync::Arc;
use rustls::client::danger::{ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, Error, RootCertStore, SignatureScheme};
use tokio_postgres_rustls::MakeRustlsConnect;

#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let password = env::var("PGPASSWORD").expect("PGPASSWORD not set");

    let mut config = ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();

    config.dangerous().set_certificate_verifier(Arc::new(NoVerifier));
    let connector = MakeRustlsConnect::new(config);

    let (client, connection) = tokio_postgres::connect(
        // Note: use "maincloud.spacetimedb.com" and sslmode=require for SpacetimeDB Cloud
        &format!(
            "host=localhost port=5432 user=any sslmode=disable dbname=quickstart-chat password={password}"
        ),
        connector,
    ).await?;

    tokio::spawn(async move { connection.await.expect("connection error") });

    println!("Running query:");
    let rows = client.simple_query("SELECT * FROM message;").await?;
    for row in rows {
        println!("Row: {:?}", row);
    }
    println!("Done.");
    Ok(())
}
```
