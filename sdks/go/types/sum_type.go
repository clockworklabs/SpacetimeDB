package types

import "github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"

// SumTypeVariant is a variant in a sum type.
type SumTypeVariant struct {
	Name          string
	AlgebraicType AlgebraicType
}

// SumType describes a sum (enum/union) type with named variants.
type SumType interface {
	bsatn.Serializable
	Variants() []SumTypeVariant
}

// NewSumType creates a SumType from the given variants.
func NewSumType(variants ...SumTypeVariant) SumType {
	return &sumType{variants: variants}
}

type sumType struct {
	variants []SumTypeVariant
}

func (s *sumType) Variants() []SumTypeVariant {
	return s.variants
}

func (s *sumType) WriteBsatn(w bsatn.Writer) {
	// SumType is encoded as array of SumTypeVariant.
	w.PutArrayLen(uint32(len(s.variants)))
	for _, v := range s.variants {
		// Each variant is a product of (name: Option<String>, algebraic_type: AlgebraicType).
		// Name is always present (Some).
		w.PutSumTag(0) // Some
		w.PutString(v.Name)
		v.AlgebraicType.WriteBsatn(w)
	}
}
