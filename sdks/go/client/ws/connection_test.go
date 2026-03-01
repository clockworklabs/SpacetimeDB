package ws_test

import (
	"testing"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/ws"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewConnection_ReturnsBuilder(t *testing.T) {
	builder := ws.NewConnection()
	require.NotNil(t, builder)
}

func TestConnectionBuilder_Chaining(t *testing.T) {
	builder := ws.NewConnection().
		WithUri("ws://localhost:3000").
		WithDatabaseName("test-db").
		WithToken("my-token").
		WithProtocol("v2.bsatn.spacetimedb")

	require.NotNil(t, builder, "builder should not be nil after chaining")
}

func TestConnectionBuilder_WithUri(t *testing.T) {
	builder := ws.NewConnection().WithUri("ws://example.com:8080")
	require.NotNil(t, builder)
}

func TestConnectionBuilder_WithDatabaseName(t *testing.T) {
	builder := ws.NewConnection().WithDatabaseName("my-database")
	require.NotNil(t, builder)
}

func TestConnectionBuilder_WithToken(t *testing.T) {
	builder := ws.NewConnection().WithToken("auth-token-abc")
	require.NotNil(t, builder)
}

func TestConnectionBuilder_WithProtocol(t *testing.T) {
	builder := ws.NewConnection().WithProtocol("v2.bsatn.spacetimedb")
	require.NotNil(t, builder)
}

func TestConnectionBuilder_AllFieldsChained(t *testing.T) {
	// Verify that all builder methods can be chained in any order
	builder := ws.NewConnection().
		WithToken("tok").
		WithProtocol("v2.bsatn.spacetimedb").
		WithDatabaseName("db").
		WithUri("ws://host:1234")

	require.NotNil(t, builder)
}

func TestConnectionBuilder_InterfaceTypes(t *testing.T) {
	// Verify the interface types exist and are distinct
	var _ ws.Connection
	var _ ws.ConnectionBuilder

	builder := ws.NewConnection()
	var cb ws.ConnectionBuilder = builder
	assert.NotNil(t, cb)
}
