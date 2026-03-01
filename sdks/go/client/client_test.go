package client_test

import (
	"context"
	"errors"
	"testing"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/client"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/protocol"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// --- Error types tests ---

func TestConnectionError_Error(t *testing.T) {
	err := &client.ConnectionError{
		Message: "failed to connect",
		Err:     errors.New("dial timeout"),
	}

	assert.Contains(t, err.Error(), "connection error")
	assert.Contains(t, err.Error(), "failed to connect")
	assert.Contains(t, err.Error(), "dial timeout")
}

func TestConnectionError_ErrorWithoutWrapped(t *testing.T) {
	err := &client.ConnectionError{
		Message: "server unreachable",
	}

	assert.Contains(t, err.Error(), "connection error")
	assert.Contains(t, err.Error(), "server unreachable")
	assert.Nil(t, err.Unwrap())
}

func TestConnectionError_Unwrap(t *testing.T) {
	inner := errors.New("underlying error")
	err := &client.ConnectionError{
		Message: "test",
		Err:     inner,
	}

	assert.Equal(t, inner, err.Unwrap())
	assert.True(t, errors.Is(err, inner))
}

func TestConnectionError_ImplementsError(t *testing.T) {
	var _ error = (*client.ConnectionError)(nil)
}

func TestReducerError_Error(t *testing.T) {
	err := &client.ReducerError{
		ReducerName: "add_user",
		Message:     "unique constraint violated",
	}

	assert.Contains(t, err.Error(), "reducer")
	assert.Contains(t, err.Error(), "add_user")
	assert.Contains(t, err.Error(), "unique constraint violated")
}

func TestReducerError_ImplementsError(t *testing.T) {
	var _ error = (*client.ReducerError)(nil)
}

func TestSubscriptionError_Error(t *testing.T) {
	err := &client.SubscriptionError{
		QuerySetID: 42,
		Message:    "table not found",
	}

	assert.Contains(t, err.Error(), "subscription")
	assert.Contains(t, err.Error(), "42")
	assert.Contains(t, err.Error(), "table not found")
}

func TestSubscriptionError_ImplementsError(t *testing.T) {
	var _ error = (*client.SubscriptionError)(nil)
}

func TestProtocolError_Error(t *testing.T) {
	err := &client.ProtocolError{
		Message: "invalid tag",
		Err:     errors.New("tag 99"),
	}

	assert.Contains(t, err.Error(), "protocol error")
	assert.Contains(t, err.Error(), "invalid tag")
	assert.Contains(t, err.Error(), "tag 99")
}

func TestProtocolError_ErrorWithoutWrapped(t *testing.T) {
	err := &client.ProtocolError{
		Message: "bad data",
	}

	assert.Contains(t, err.Error(), "protocol error")
	assert.Contains(t, err.Error(), "bad data")
	assert.Nil(t, err.Unwrap())
}

func TestProtocolError_Unwrap(t *testing.T) {
	inner := errors.New("decode failed")
	err := &client.ProtocolError{
		Message: "test",
		Err:     inner,
	}

	assert.Equal(t, inner, err.Unwrap())
	assert.True(t, errors.Is(err, inner))
}

func TestProtocolError_ImplementsError(t *testing.T) {
	var _ error = (*client.ProtocolError)(nil)
}

// --- DbConnectionBuilder tests ---

func TestNewDbConnection_ReturnsBuilder(t *testing.T) {
	builder := client.NewDbConnection()
	require.NotNil(t, builder)
}

func TestDbConnectionBuilder_Chaining(t *testing.T) {
	var connectCalled bool
	var errorCalled bool
	var disconnectCalled bool

	builder := client.NewDbConnection().
		WithUri("ws://localhost:3000").
		WithDatabaseName("test-db").
		WithToken("my-token").
		OnConnect(func(conn client.DbConnection, identity types.Identity, token string) {
			connectCalled = true
		}).
		OnConnectError(func(err error) {
			errorCalled = true
		}).
		OnDisconnect(func(conn client.DbConnection, err error) {
			disconnectCalled = true
		})

	require.NotNil(t, builder, "builder should not be nil after chaining")

	// We cannot call Build without a real server, but we verify the builder is valid
	// and chaining works. The callbacks aren't called yet.
	assert.False(t, connectCalled)
	assert.False(t, errorCalled)
	assert.False(t, disconnectCalled)
}

func TestDbConnectionBuilder_WithCompression(t *testing.T) {
	builder := client.NewDbConnection().
		WithUri("ws://localhost:3000").
		WithDatabaseName("test-db").
		WithCompression(protocol.CompressionBrotli)

	require.NotNil(t, builder, "builder should not be nil after WithCompression")
}

func TestDbConnectionBuilder_BuildFailsWithBadUri(t *testing.T) {
	var errorCalled bool
	var capturedErr error

	builder := client.NewDbConnection().
		WithUri("ws://localhost:99999-invalid").
		WithDatabaseName("nonexistent-db").
		OnConnectError(func(err error) {
			errorCalled = true
			capturedErr = err
		})

	ctx := context.Background()
	conn, err := builder.Build(ctx)

	// Build should fail because there's no server
	require.Error(t, err)
	assert.Nil(t, conn)
	assert.True(t, errorCalled, "OnConnectError should have been called")
	assert.NotNil(t, capturedErr)

	// The returned error should be a ConnectionError
	var connErr *client.ConnectionError
	assert.True(t, errors.As(err, &connErr), "error should be a *ConnectionError")
	assert.Contains(t, connErr.Error(), "connection error")
}

func TestDbConnectionBuilder_BuildFailsNoOnConnectError(t *testing.T) {
	// Build without OnConnectError handler -- should still return an error
	builder := client.NewDbConnection().
		WithUri("ws://localhost:99999-invalid").
		WithDatabaseName("nonexistent-db")

	ctx := context.Background()
	conn, err := builder.Build(ctx)

	require.Error(t, err)
	assert.Nil(t, conn)
}

// --- EventContext tests ---

func TestEventContext_Fields(t *testing.T) {
	identity := types.NewIdentity([32]byte{0x01})
	connID := types.NewConnectionId([16]byte{0x02})
	ts := types.NewTimestamp(1234567890)

	ctx := client.EventContext{
		Identity:     identity,
		ConnectionID: connID,
		Timestamp:    ts,
		Conn:         nil, // we don't have a real connection in unit tests
	}

	assert.Equal(t, identity, ctx.Identity)
	assert.Equal(t, connID, ctx.ConnectionID)
	assert.Equal(t, ts, ctx.Timestamp)
	assert.Nil(t, ctx.Conn)
}

func TestReducerEventContext_Fields(t *testing.T) {
	identity := types.NewIdentity([32]byte{0x01})
	connID := types.NewConnectionId([16]byte{0x02})
	ts := types.NewTimestamp(5000)

	ctx := client.ReducerEventContext{
		EventContext: client.EventContext{
			Identity:     identity,
			ConnectionID: connID,
			Timestamp:    ts,
		},
		ReducerName: "add_player",
		Status:      "committed",
		ErrMessage:  "",
	}

	assert.Equal(t, identity, ctx.Identity)
	assert.Equal(t, "add_player", ctx.ReducerName)
	assert.Equal(t, "committed", ctx.Status)
	assert.Empty(t, ctx.ErrMessage)
}

func TestReducerEventContext_WithError(t *testing.T) {
	ctx := client.ReducerEventContext{
		ReducerName: "delete_user",
		Status:      "failed",
		ErrMessage:  "user not found",
	}

	assert.Equal(t, "delete_user", ctx.ReducerName)
	assert.Equal(t, "failed", ctx.Status)
	assert.Equal(t, "user not found", ctx.ErrMessage)
}

func TestErrorContext_Fields(t *testing.T) {
	inner := errors.New("something went wrong")
	ctx := client.ErrorContext{Err: inner}

	assert.Equal(t, inner, ctx.Err)
}

// --- SubscriptionBuilder/Handle interface tests ---

func TestSubscriptionBuilder_Interface(t *testing.T) {
	// Verify the interface shapes exist and are accessible
	var _ client.SubscriptionBuilder
	var _ client.SubscriptionHandle
}

func TestCallbackID_Type(t *testing.T) {
	var id client.CallbackID = 42
	assert.Equal(t, client.CallbackID(42), id)
}
