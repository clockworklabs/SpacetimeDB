package bsatn

// Variant represents an enum variant (tagged union) with index and optional payload.
// Index follows declaration order in the original Rust enum.
// Value may be nil for unit variants.
type Variant struct {
	Index uint32
	Value interface{}
}

func NewUnitVariant(idx uint32) Variant {
	return Variant{Index: idx}
}

func NewVariant(idx uint32, val interface{}) Variant {
	return Variant{Index: idx, Value: val}
}
