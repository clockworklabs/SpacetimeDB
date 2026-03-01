package types

import (
	"encoding/binary"
	"math/big"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// Uint128 represents an unsigned 128-bit integer stored as 16 bytes little-endian.
type Uint128 interface {
	bsatn.Serializable
	Bytes() [16]byte
	Lo() uint64
	Hi() uint64
	IsZero() bool
	String() string
}

// NewUint128 creates a Uint128 from low and high 64-bit components.
func NewUint128(lo, hi uint64) Uint128 {
	var b [16]byte
	binary.LittleEndian.PutUint64(b[0:8], lo)
	binary.LittleEndian.PutUint64(b[8:16], hi)
	return &uint128{data: b}
}

// NewUint128FromBytes creates a Uint128 from a 16-byte little-endian array.
func NewUint128FromBytes(b [16]byte) Uint128 {
	return &uint128{data: b}
}

// ReadUint128 reads a Uint128 from a BSATN reader (16 bytes little-endian).
func ReadUint128(r bsatn.Reader) (Uint128, error) {
	b, err := r.GetBytes(16)
	if err != nil {
		return nil, err
	}
	var data [16]byte
	copy(data[:], b)
	return &uint128{data: data}, nil
}

type uint128 struct {
	data [16]byte
}

func (u *uint128) WriteBsatn(w bsatn.Writer) {
	w.PutBytes(u.data[:])
}

func (u *uint128) Bytes() [16]byte { return u.data }

func (u *uint128) Lo() uint64 {
	return binary.LittleEndian.Uint64(u.data[0:8])
}

func (u *uint128) Hi() uint64 {
	return binary.LittleEndian.Uint64(u.data[8:16])
}

func (u *uint128) IsZero() bool {
	return u.data == [16]byte{}
}

func (u *uint128) String() string {
	// Convert LE bytes to big-endian for math/big, then format as decimal.
	var be [16]byte
	for i := 0; i < 16; i++ {
		be[i] = u.data[15-i]
	}
	var n big.Int
	n.SetBytes(be[:])
	return n.String()
}
