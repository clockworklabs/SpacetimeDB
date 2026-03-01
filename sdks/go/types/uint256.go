package types

import (
	"encoding/binary"
	"math/big"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// Uint256 represents an unsigned 256-bit integer stored as 32 bytes little-endian.
type Uint256 interface {
	bsatn.Serializable
	Bytes() [32]byte
	IsZero() bool
	String() string
}

// NewUint256 creates a Uint256 from a 32-byte little-endian array.
func NewUint256(b [32]byte) Uint256 {
	return &uint256{data: b}
}

// NewUint256FromU64s creates a Uint256 from four uint64 values stored little-endian:
// a=bytes[0:8], b=bytes[8:16], c=bytes[16:24], d=bytes[24:32].
func NewUint256FromU64s(a, b, c, d uint64) Uint256 {
	var buf [32]byte
	binary.LittleEndian.PutUint64(buf[0:8], a)
	binary.LittleEndian.PutUint64(buf[8:16], b)
	binary.LittleEndian.PutUint64(buf[16:24], c)
	binary.LittleEndian.PutUint64(buf[24:32], d)
	return &uint256{data: buf}
}

// ReadUint256 reads a Uint256 from a BSATN reader (32 bytes little-endian).
func ReadUint256(r bsatn.Reader) (Uint256, error) {
	b, err := r.GetBytes(32)
	if err != nil {
		return nil, err
	}
	var data [32]byte
	copy(data[:], b)
	return &uint256{data: data}, nil
}

type uint256 struct {
	data [32]byte
}

func (u *uint256) WriteBsatn(w bsatn.Writer) {
	w.PutBytes(u.data[:])
}

func (u *uint256) Bytes() [32]byte { return u.data }

func (u *uint256) IsZero() bool {
	return u.data == [32]byte{}
}

func (u *uint256) String() string {
	// Convert LE bytes to big-endian for math/big, then format as decimal.
	var be [32]byte
	for i := 0; i < 32; i++ {
		be[i] = u.data[31-i]
	}
	var n big.Int
	n.SetBytes(be[:])
	return n.String()
}
