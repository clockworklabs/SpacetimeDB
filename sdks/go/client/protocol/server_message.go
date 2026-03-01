package protocol

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ServerMessage represents a message sent from the server to the client.
// It is a BSATN sum type with variants for each response kind.
type ServerMessage interface {
	serverMessageTag() uint8
}

// InitialConnection is sent upon a successful connection.
// Tag 0 in the ServerMessage sum type.
type InitialConnection struct {
	Identity     types.Identity
	ConnectionID types.ConnectionId
	Token        string
}

func (*InitialConnection) serverMessageTag() uint8 { return 0 }

// SubscribeApplied is sent in response to a Subscribe, containing initial matching rows.
// Tag 1 in the ServerMessage sum type.
type SubscribeApplied struct {
	RequestID  uint32
	QuerySetID uint32
	Rows       QueryRows
}

func (*SubscribeApplied) serverMessageTag() uint8 { return 1 }

// UnsubscribeApplied confirms that a subscription has been removed.
// Tag 2 in the ServerMessage sum type.
type UnsubscribeApplied struct {
	RequestID  uint32
	QuerySetID uint32
	Rows       *QueryRows // Option<QueryRows>: nil means None
}

func (*UnsubscribeApplied) serverMessageTag() uint8 { return 2 }

// SubscriptionError notifies the client of a subscription failure.
// Tag 3 in the ServerMessage sum type.
type SubscriptionError struct {
	RequestID  *uint32 // Option<u32>: nil means None
	QuerySetID uint32
	Error      string
}

func (*SubscriptionError) serverMessageTag() uint8 { return 3 }

// TransactionUpdate is sent after a committed transaction,
// containing query set updates for affected subscriptions.
// Tag 4 in the ServerMessage sum type.
type TransactionUpdate struct {
	QuerySets []QuerySetUpdate
}

func (*TransactionUpdate) serverMessageTag() uint8 { return 4 }

// OneOffQueryResult is sent in response to a OneOffQuery.
// Tag 5 in the ServerMessage sum type.
// Result is modeled as: non-nil QueryRows = Ok, non-empty ErrorMsg = Err.
type OneOffQueryResult struct {
	RequestID uint32
	ResultOk  *QueryRows // non-nil if the query succeeded
	ResultErr string     // non-empty if the query failed
}

func (*OneOffQueryResult) serverMessageTag() uint8 { return 5 }

// ReadServerMessage reads a ServerMessage from a BSATN reader by dispatching on the sum type tag.
func ReadServerMessage(r bsatn.Reader) (ServerMessage, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}

	switch tag {
	case 0:
		return readInitialConnection(r)
	case 1:
		return readSubscribeApplied(r)
	case 2:
		return readUnsubscribeApplied(r)
	case 3:
		return readSubscriptionError(r)
	case 4:
		return readTransactionUpdate(r)
	case 5:
		return readOneOffQueryResult(r)
	case 6:
		return readReducerResult(r)
	case 7:
		return readProcedureResult(r)
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: tag, SumName: "ServerMessage"}
	}
}
