# SpacetimeDB Go Bindings

This package provides Go bindings for SpacetimeDB, allowing Go applications to interact with SpacetimeDB databases and WASM modules.

## Project Structure

The project follows the standard Go project layout:

```
.
├── cmd/                    # Main applications
│   └── spacetimedb/       # SpacetimeDB CLI and tools
├── internal/              # Private application and library code
│   ├── bsatn/            # BSATN encoding/decoding
│   ├── db/               # Database operations
│   ├── errors/           # Error handling
│   ├── monitoring/       # Monitoring and metrics
│   ├── performance/      # Performance optimizations
│   ├── security/         # Security features
│   ├── types/            # Core types
│   └── wasm/             # WASM runtime integration
├── pkg/                  # Public library code
│   └── spacetimedb/      # Public API
├── docs/                 # Documentation
├── examples/             # Example applications
├── benchmarks/           # Benchmarking code
└── tests/               # Integration tests
```

## Installation

```bash
go get github.com/clockworklabs/SpacetimeDB/crates/bindings-go
```

## Usage

```go
package main

import (
    "fmt"
    "log"

    "github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb"
)

func main() {
    client, err := spacetimedb.NewClient()
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    // Use the client to interact with SpacetimeDB
}
```

## Development

### Prerequisites

- Go 1.21 or later
- WABT (WebAssembly Binary Toolkit)
- Rust toolchain (for building WASM modules)

### Building

```bash
go build ./cmd/spacetimedb
```

### Testing

```bash
go test ./...
```

### Running Benchmarks

```bash
go test -bench=. ./benchmarks/...
```

## License

This project is licensed under the same license as SpacetimeDB. 