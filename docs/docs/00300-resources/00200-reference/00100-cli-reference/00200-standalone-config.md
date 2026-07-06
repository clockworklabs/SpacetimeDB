---
title: Standalone Configuration
slug: /cli-reference/standalone-config
---


A local database instance (as started by `spacetime start`) can be configured in `{data-dir}/config.toml`, where `{data-dir}` is the database's data directory. This directory is printed when you run `spacetime start`:

<pre class="shiki"><span>spacetimedb-standalone version: 1.0.0
spacetimedb-standalone path: /home/user/.local/share/spacetime/bin/1.0.0/spacetimedb-standalone
database running in data directory <b>/home/user/.local/share/spacetime/data</b></span></pre>

On Linux and macOS, this directory is by default `~/.local/share/spacetime/data`. On Windows, it's `%LOCALAPPDATA%\SpacetimeDB\data`.

## `config.toml`

- [`certificate-authority`](#certificate-authority)

- [`logs`](#logs)

- [`commitlog`](#commitlog)

- [`hot-backup`](#hot-backup)

- [`scheduled-backup`](#scheduled-backup)

- [`websocket`](#websocket)

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

### `commitlog`

```toml
[commitlog]
log-format-version = 1
max-segment-size = 1073741824 # 1GiB
offset-index-interval-bytes = 4096
offset-index-require-segment-fsync = true
preallocate-segments = false
write-buffer-size = 131072 # 128KiB
```

The `commitlog` table configures local durability. These settings are advanced and may affect recovery behavior, disk usage, memory usage, and write throughput. Omitted fields use the server's built-in defaults.

#### `commitlog.log-format-version`

The maximum supported commitlog format version, also used for writing.

::::caution
This setting should not normally be changed from the commitlog crate's default. A reason to change it could be to make the server accept an older, incompatible commitlog.
::::

#### `commitlog.max-segment-size`

The maximum size in bytes to which commitlog segments should be allowed to grow.

#### `commitlog.offset-index-interval-bytes`

Number of bytes written to the commitlog after which an entry is added to the offset index.

#### `commitlog.offset-index-require-segment-fsync`

If `true`, require that the segment must be synced to disk before an index entry is added.

Setting this to `false` will update the index every `offset-index-interval-bytes`, even if the commitlog was not synced. This means that the index could contain non-existent entries in the event of a crash.

Setting this to `true` will update the index when the commitlog is synced, and `offset-index-interval-bytes` have been written. This means that the index could contain fewer index entries than strictly every `offset-index-interval-bytes`.

::::note
The commitlog operates correctly under both settings, but the choice can have performance implications.
::::

#### `commitlog.preallocate-segments`

If `true`, preallocate disk space for commitlog segments up to `commitlog.max-segment-size`. This has no effect unless commitlog fallocate support is enabled.

#### `commitlog.write-buffer-size`

Size in bytes of the memory buffer holding commit data before flushing to storage.

### `hot-backup`

```toml
[hot-backup]
root-dir = "/var/backups/stdb"
```

The `hot-backup` section configures server-side backups created through `spacetime backup create` or the HTTP backup endpoint.

#### `hot-backup.root-dir`

An absolute server path that acts as the root directory for CLI/HTTP-triggered hot backups. Backup creation requests choose an output directory relative to this root. Omit `root-dir` to disable CLI/HTTP-triggered hot backups.

### `scheduled-backup`

```toml
[scheduled-backup]
database = "mydb"
output-dir = "/var/backups/stdb"
interval = "1h"
keep-last = 24
```

The `scheduled-backup` section configures a background task that periodically backs up one local database.

#### `scheduled-backup.database`

The database name or identity to back up.

#### `scheduled-backup.output-dir`

An absolute server path where scheduled backups are created. Each backup is written into a timestamped `stdb-*` subdirectory.

#### `scheduled-backup.interval`

How often to create a backup. Values are strings of any format the [`humantime`] crate can parse, such as `"15m"`, `"1h"`, or `"1day"`.

#### `scheduled-backup.keep-last`

The number of complete scheduled backups to retain. Incomplete `stdb-*` directories are ignored during pruning and do not count toward this limit. A failed scheduled run removes its own incomplete output directory; other manifest-less directories are preserved because they may belong to a concurrent manual or HTTP-triggered backup. Omit `keep-last` to keep all complete scheduled backups.

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
