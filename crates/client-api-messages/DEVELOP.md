# Generate client bindings for the WebSocket message schema

After changing the WebSocket message schema, generate client bindings for them as follows.

In this directory:

```sh
cargo run --example get_ws_schema > ws_schema.json
spacetime generate --lang <SDK lang> \
  --out-dir <sdk WebSocket schema bindings dir>
  --json-module ws_schema.json
```
