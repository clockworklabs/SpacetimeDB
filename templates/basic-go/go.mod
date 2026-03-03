module example.com/my-spacetimedb-client

go 1.23

require github.com/clockworklabs/SpacetimeDB/sdks/go v0.0.0

require (
	github.com/andybalholm/brotli v1.1.1 // indirect
	github.com/coder/websocket v1.8.12 // indirect
	github.com/puzpuzpuz/xsync/v3 v3.5.1 // indirect
)

replace github.com/clockworklabs/SpacetimeDB/sdks/go => ../../sdks/go
