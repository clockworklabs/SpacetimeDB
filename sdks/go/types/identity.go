package types

import (
	"encoding/binary"
	"encoding/hex"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// Identity is a 32-byte value representing a user identity.
// In BSATN it is encoded as a product containing a u256 (32 raw bytes).
type Identity interface {
	bsatn.Serializable
	Bytes() [32]byte
	IsZero() bool
	String() string
}

// NewIdentity creates an Identity from a 32-byte array.
func NewIdentity(b [32]byte) Identity {
	return &identity{data: b}
}

// NewIdentityFromU64s reconstructs an Identity from 4 uint64 values (WASM ABI format).
// Each u64 is in little-endian byte order.
func NewIdentityFromU64s(s0, s1, s2, s3 uint64) Identity {
	var b [32]byte
	binary.LittleEndian.PutUint64(b[0:8], s0)
	binary.LittleEndian.PutUint64(b[8:16], s1)
	binary.LittleEndian.PutUint64(b[16:24], s2)
	binary.LittleEndian.PutUint64(b[24:32], s3)
	return &identity{data: b}
}

// ReadIdentity reads an Identity from a BSATN reader (32 bytes).
func ReadIdentity(r bsatn.Reader) (Identity, error) {
	b, err := r.GetBytes(32)
	if err != nil {
		return nil, err
	}
	var data [32]byte
	copy(data[:], b)
	return &identity{data: data}, nil
}

type identity struct {
	data [32]byte
}

func (id *identity) WriteBsatn(w bsatn.Writer) {
	w.PutBytes(id.data[:])
}

func (id *identity) Bytes() [32]byte { return id.data }

func (id *identity) IsZero() bool {
	return id.data == [32]byte{}
}

func (id *identity) String() string {
	return hex.EncodeToString(id.data[:])
}
