package types

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// Typespace is a registry of types indexed by TypeRef.
type Typespace interface {
	bsatn.Serializable
	Add(at AlgebraicType) TypeRef
	// Reserve allocates a slot and returns a TypeRef. The slot must be filled
	// later via Set. This supports forward-referencing for recursive types.
	Reserve() TypeRef
	// Set updates the AlgebraicType at an existing TypeRef (from Add or Reserve).
	Set(ref TypeRef, at AlgebraicType)
	Get(ref TypeRef) AlgebraicType
	Len() int
}

// NewTypespace creates an empty Typespace.
func NewTypespace() Typespace {
	return &typespace{}
}

type typespace struct {
	types []AlgebraicType
}

func (t *typespace) Add(at AlgebraicType) TypeRef {
	ref := TypeRef(len(t.types))
	t.types = append(t.types, at)
	return ref
}

func (t *typespace) Reserve() TypeRef {
	ref := TypeRef(len(t.types))
	t.types = append(t.types, nil)
	return ref
}

func (t *typespace) Set(ref TypeRef, at AlgebraicType) {
	t.types[ref] = at
}

func (t *typespace) Get(ref TypeRef) AlgebraicType {
	return t.types[ref]
}

func (t *typespace) Len() int {
	return len(t.types)
}

func (t *typespace) WriteBsatn(w bsatn.Writer) {
	// Typespace is encoded as array of AlgebraicType.
	w.PutArrayLen(uint32(len(t.types)))
	for _, at := range t.types {
		at.WriteBsatn(w)
	}
}
