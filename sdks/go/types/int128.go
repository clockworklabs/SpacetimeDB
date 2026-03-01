package types

import (
	"encoding/binary"
	"math/big"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
)

// Int128 represents a signed 128-bit integer stored as 16 bytes little-endian (two's complement).
type Int128 interface {
	bsatn.Serializable
	Bytes() [16]byte
	Lo() uint64
	Hi() uint64
	IsZero() bool
	String() string
}

// NewInt128 creates an Int128 from low and high 64-bit components.
func NewInt128(lo, hi uint64) Int128 {
	var b [16]byte
	binary.LittleEndian.PutUint64(b[0:8], lo)
	binary.LittleEndian.PutUint64(b[8:16], hi)
	return &int128{data: b}
}

// NewInt128FromBytes creates an Int128 from a 16-byte little-endian array (two's complement).
func NewInt128FromBytes(b [16]byte) Int128 {
	return &int128{data: b}
}

// ReadInt128 reads an Int128 from a BSATN reader (16 bytes little-endian).
func ReadInt128(r bsatn.Reader) (Int128, error) {
	b, err := r.GetBytes(16)
	if err != nil {
		return nil, err
	}
	var data [16]byte
	copy(data[:], b)
	return &int128{data: data}, nil
}

type int128 struct {
	data [16]byte
}

func (i *int128) WriteBsatn(w bsatn.Writer) {
	w.PutBytes(i.data[:])
}

func (i *int128) Bytes() [16]byte { return i.data }

func (i *int128) Lo() uint64 {
	return binary.LittleEndian.Uint64(i.data[0:8])
}

func (i *int128) Hi() uint64 {
	return binary.LittleEndian.Uint64(i.data[8:16])
}

func (i *int128) IsZero() bool {
	return i.data == [16]byte{}
}

func (i *int128) String() string {
	// Convert LE bytes to big-endian for math/big.
	var be [16]byte
	for idx := 0; idx < 16; idx++ {
		be[idx] = i.data[15-idx]
	}
	var n big.Int
	if be[0]&0x80 != 0 {
		// Negative two's complement: value = unsigned_value - 2^128.
		n.SetBytes(be[:])
		var mod big.Int
		mod.Lsh(big.NewInt(1), 128)
		n.Sub(&n, &mod)
	} else {
		n.SetBytes(be[:])
	}
	return n.String()
}
