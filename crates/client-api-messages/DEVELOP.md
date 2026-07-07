# Generate client bindings for the WebSocket message schema

After changing the WebSocket message schema, generate client bindings for them as follows.

In this directory:

```sh
cargo run --example get_ws_schema > ws_schema.json
spacetime generate -p spacetimedb-cli --lang <SDK lang> \
  --out-dir <sdk WebSocket schema bindings dir> \
  --module-def ws_schema.json
```

For the v2 WebSocket protocol schema:

```sh
cargo run --example get_ws_schema_v2 > ws_schema_v2.json
spacetime generate -p spacetimedb-cli --lang <SDK lang> \
  --out-dir <sdk WebSocket schema bindings dir> \
  --module-def ws_schema_v2.json
```

Note, the v3 WebSocket protocol does not have a separate schema.
It reuses the v2 message schema and only changes the websocket binary framing.
In v2 for example, a WebSocket frame contained a single BSATN-encoded v2 message,
but in v3, a single WebSocket frame may contain a batch of one or more v2 messages.
