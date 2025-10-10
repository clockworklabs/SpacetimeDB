---
title: Standalone Configuration
slug: /cli-reference/standalone-config
---

# `spacetimedb-standalone` configuration

A local database instance (as started by `spacetime start`) can be configured in `{data-dir}/config.toml`, where `{data-dir}` is the database's data directory. This directory is printed when you run `spacetime start`:

<pre class="shiki"><span>spacetimedb-standalone version: 1.0.0
spacetimedb-standalone path: /home/user/.local/share/spacetime/bin/1.0.0/spacetimedb-standalone
database running in data directory <b>/home/user/.local/share/spacetime/data</b></span></pre>

On Linux and macOS, this directory is by default `~/.local/share/spacetime/data`. On Windows, it's `%LOCALAPPDATA%\SpacetimeDB\data`.

## `config.toml`

- [`certificate-authority`](#certificate-authority)

- [`logs`](#logs)

### `certificate-authority`

```toml
[certificate-authority]
jwt-priv-key-path = "/path/to/id_ecdsas"
jwt-pub-key-path = "/path/to/id_ecdsas.pub"
```

The `certificate-authority` table lets you configure the public and private keys used by the database to sign tokens.

### `logs`

```toml
[logs]
level = "error"
directives = [
    "spacetimedb=warn",
    "spacetimedb_standalone=info",
]
```

#### `logs.level`

Can be one of `"error"`, `"warn"`, `"info"`, `"debug"`, `"trace"`, or `"off"`, case-insensitive. Only log messages of the specified level or higher will be output; e.g. if set to `warn`, only `error` and `warn`-level messages will be logged.

#### `logs.directives`

A list of filtering directives controlling what messages get logged, which overwrite the global [`logs.level`](#logslevel). See [`tracing documentation`](https://docs.rs/tracing-subscriber/0.3/tracing_subscriber/filter/struct.EnvFilter.html#directives) for syntax. Note that this is primarily intended as a debugging tool, and log message fields and targets are not considered stable.

### `websocket`

```toml
[websocket]
ping-interval = "15s"
idle-timeout = "30s"
close-handshake-timeout = "250ms"
incoming-queue-length = 2048
```

#### `websocket.ping-interval`

Interval at which the server will send `Ping` frames to keep the connection alive.
Should be smaller than `websocket.idle-timeout` to be effective.

Values are strings of any format the [`humantime`] crate can parse.

#### `websocket.idle-timeout`

If the server hasn't received any data from the client (including `Pong` responses to previous `Ping`s it sent), it will consider the client unresponsive and close the connection.
Should be greater than `websocket.ping-interval` to be effective.

Values are strings of any format the [`humantime`] crate can parse.

#### `websocket.close-handshake-timeout`

Time the server waits for the client to respond to a graceful connection close. If the client doesn't respond within this timeout, the connection is dropped.

Values are strings of any format the [`humantime`] crate can parse.

#### `websocket.incoming-queue-length`

Maximum number of client messages the server will queue up in case it is not able to process them quickly enough. When the queue length exceeds this value, the server will start disconnecting clients.
Note that the limit is per client, not across all clients of a particular database.

[`humantime`]: https://crates.io/crates/humantime
