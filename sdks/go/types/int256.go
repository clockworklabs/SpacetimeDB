package types

import (
	"encoding/binary"
	"math/big"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// Int256 represents a signed 256-bit integer stored as 32 bytes little-endian (two's complement).
type Int256 interface {
	bsatn.Serializable
	Bytes() [32]byte
	IsZero() bool
	String() string
}

// NewInt256 creates an Int256 from a 32-byte little-endian array (two's complement).
func NewInt256(b [32]byte) Int256 {
	return &int256{data: b}
}

// NewInt256FromU64s creates an Int256 from four uint64 values stored little-endian:
// a=bytes[0:8], b=bytes[8:16], c=bytes[16:24], d=bytes[24:32].
func NewInt256FromU64s(a, b, c, d uint64) Int256 {
	var buf [32]byte
	binary.LittleEndian.PutUint64(buf[0:8], a)
	binary.LittleEndian.PutUint64(buf[8:16], b)
	binary.LittleEndian.PutUint64(buf[16:24], c)
	binary.LittleEndian.PutUint64(buf[24:32], d)
	return &int256{data: buf}
}

// ReadInt256 reads an Int256 from a BSATN reader (32 bytes little-endian).
func ReadInt256(r bsatn.Reader) (Int256, error) {
	b, err := r.GetBytes(32)
	if err != nil {
		return nil, err
	}
	var data [32]byte
	copy(data[:], b)
	return &int256{data: data}, nil
}

type int256 struct {
	data [32]byte
}

func (i *int256) WriteBsatn(w bsatn.Writer) {
	w.PutBytes(i.data[:])
}

func (i *int256) Bytes() [32]byte { return i.data }

func (i *int256) IsZero() bool {
	return i.data == [32]byte{}
}

func (i *int256) String() string {
	// Convert LE bytes to big-endian for math/big.
	var be [32]byte
	for idx := 0; idx < 32; idx++ {
		be[idx] = i.data[31-idx]
	}
	var n big.Int
	if be[0]&0x80 != 0 {
		// Negative two's complement: value = unsigned_value - 2^256.
		n.SetBytes(be[:])
		var mod big.Int
		mod.Lsh(big.NewInt(1), 256)
		n.Sub(&n, &mod)
	} else {
		n.SetBytes(be[:])
	}
	return n.String()
}
