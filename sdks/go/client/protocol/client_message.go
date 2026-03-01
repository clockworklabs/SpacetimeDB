package protocol

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// ClientMessage represents a message sent from client to server.
// It is a BSATN sum type with variants for each message kind.
type ClientMessage interface {
	bsatn.Serializable
	clientMessageTag() uint8
}

// Subscribe requests a new subscription to a set of queries.
// Tag 0 in the ClientMessage sum type.
type Subscribe struct {
	RequestID    uint32
	QuerySetID   uint32
	QueryStrings []string
}

func (*Subscribe) clientMessageTag() uint8 { return 0 }

func (s *Subscribe) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(0)
	w.PutU32(s.RequestID)
	// QuerySetId is a product with a single u32 field
	w.PutU32(s.QuerySetID)
	// query_strings: Box<[Box<str>]> serialized as array of strings
	w.PutArrayLen(uint32(len(s.QueryStrings)))
	for _, q := range s.QueryStrings {
		w.PutString(q)
	}
}

// UnsubscribeFlags controls the behavior of an Unsubscribe request.
type UnsubscribeFlags uint8

const (
	// UnsubscribeFlagsDefault is the default unsubscribe behavior.
	UnsubscribeFlagsDefault UnsubscribeFlags = 0
	// UnsubscribeFlagsSendDroppedRows requests the server send dropped rows.
	UnsubscribeFlagsSendDroppedRows UnsubscribeFlags = 1
)

// Unsubscribe removes a previously-registered subscription.
// Tag 1 in the ClientMessage sum type.
type Unsubscribe struct {
	RequestID  uint32
	QuerySetID uint32
	Flags      UnsubscribeFlags
}

func (*Unsubscribe) clientMessageTag() uint8 { return 1 }

func (u *Unsubscribe) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(1)
	w.PutU32(u.RequestID)
	w.PutU32(u.QuerySetID)
	// UnsubscribeFlags is a sum type enum: tag byte only, empty product payload
	w.PutSumTag(uint8(u.Flags))
}

// OneOffQuery runs a query once without subscribing to updates.
// Tag 2 in the ClientMessage sum type.
type OneOffQuery struct {
	RequestID   uint32
	QueryString string
}

func (*OneOffQuery) clientMessageTag() uint8 { return 2 }

func (o *OneOffQuery) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(2)
	w.PutU32(o.RequestID)
	w.PutString(o.QueryString)
}

// CallReducer invokes a reducer (transactional database function).
// Tag 3 in the ClientMessage sum type.
type CallReducer struct {
	RequestID uint32
	Flags     uint8
	Reducer   string
	Args      []byte
}

func (*CallReducer) clientMessageTag() uint8 { return 3 }

func (c *CallReducer) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(3)
	w.PutU32(c.RequestID)
	// CallReducerFlags serializes as a plain u8
	w.PutU8(c.Flags)
	w.PutString(c.Reducer)
	// args: Bytes serialized as byte array (u32 len + raw bytes)
	bsatn.WriteByteArray(w, c.Args)
}

// CallProcedure invokes a procedure (non-transactional database function).
// Tag 4 in the ClientMessage sum type.
type CallProcedure struct {
	RequestID uint32
	Flags     uint8
	Procedure string
	Args      []byte
}

func (*CallProcedure) clientMessageTag() uint8 { return 4 }

func (c *CallProcedure) WriteBsatn(w bsatn.Writer) {
	w.PutSumTag(4)
	w.PutU32(c.RequestID)
	// CallProcedureFlags serializes as a plain u8
	w.PutU8(c.Flags)
	w.PutString(c.Procedure)
	// args: Bytes serialized as byte array (u32 len + raw bytes)
	bsatn.WriteByteArray(w, c.Args)
}
