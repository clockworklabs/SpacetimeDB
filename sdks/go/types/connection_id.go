package types

import (
	"encoding/binary"
	"encoding/hex"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// ConnectionId is a 16-byte value identifying a client connection.
// In BSATN it is encoded as a product-wrapped u128 (16 raw bytes).
type ConnectionId interface {
	bsatn.Serializable
	Bytes() [16]byte
	IsZero() bool
	String() string
}

// NewConnectionId creates a ConnectionId from a 16-byte array.
func NewConnectionId(b [16]byte) ConnectionId {
	return &connectionId{data: b}
}

// NewConnectionIdFromU64s reconstructs a ConnectionId from 2 uint64 values (WASM ABI format).
// Each u64 is in little-endian byte order.
func NewConnectionIdFromU64s(c0, c1 uint64) ConnectionId {
	var b [16]byte
	binary.LittleEndian.PutUint64(b[0:8], c0)
	binary.LittleEndian.PutUint64(b[8:16], c1)
	return &connectionId{data: b}
}

// ReadConnectionId reads a ConnectionId from a BSATN reader (16 bytes).
func ReadConnectionId(r bsatn.Reader) (ConnectionId, error) {
	b, err := r.GetBytes(16)
	if err != nil {
		return nil, err
	}
	var data [16]byte
	copy(data[:], b)
	return &connectionId{data: data}, nil
}

type connectionId struct {
	data [16]byte
}

func (c *connectionId) WriteBsatn(w bsatn.Writer) {
	w.PutBytes(c.data[:])
}

func (c *connectionId) Bytes() [16]byte { return c.data }

func (c *connectionId) IsZero() bool {
	return c.data == [16]byte{}
}

func (c *connectionId) String() string {
	return hex.EncodeToString(c.data[:])
}
