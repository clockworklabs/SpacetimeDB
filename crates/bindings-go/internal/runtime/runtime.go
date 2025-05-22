package runtime

import wz "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/spacetimedb/wasm"

// Runtime is an alias to the real WASM runtime defined in spacetimedb/wasm package.
// Keeping this alias lets existing code keep importing internal/runtime while
// sharing the single implementation.
type Runtime = wz.Runtime
