package bsatn

// Algebraic type descriptions (SATS meta-type)
// This mirrors the Rust structures that SpacetimeDB uses internally so that the
// Go bindings can encode / decode type descriptions in BSATN.
//
// NOTE:  This is *value*‐level data.  We are not implementing a full-fledged
// static type checker – all we need is the ability to round-trip the
// description structs to/from BSATN so that tables and schemas coming from the
// server can be read.

// atKind is the discriminant (variant index) used by Rust when it encodes an
// `AlgebraicType` value as a `Sum`.  The order MUST stay in sync with
// crates/sats/src/algebraic_type.rs::MetaType implementation or decoding will
// fail.
//
// Variant layout:
// 0  ref(u32)
// 1  sum(SumType)
// 2  product(ProductType)
// 3  array(ArrayType)
// 4  string
// 5  bool
// 6  i8  7  u8  8  i16  9  u16  10 i32  11 u32
// 12 i64 13 u64 14 i128 15 u128 16 i256 17 u256
// 18 f32 19 f64
//
// We define constants up to f64 even though Go does not have native 128/256-bit
// ints – those variants are still kept so that we can *skip* them when we see
// them on the wire.

type atKind uint32

const (
	atRef atKind = iota
	atSum
	atProduct
	atArray
	atString
	atBool
	atI8
	atU8
	atI16
	atU16
	atI32
	atU32
	atI64
	atU64
	atI128
	atU128
	atI256
	atU256
	atF32
	atF64
)

// AlgebraicType is the union.
type AlgebraicType struct {
	Kind    atKind       `bsatn:"-"`
	Ref     uint32       `bsatn:"ref,omitempty"`
	Sum     *SumType     `bsatn:"sum,omitempty"`
	Product *ProductType `bsatn:"product,omitempty"`
	Array   *ArrayType   `bsatn:"array,omitempty"`
	// Unit variants (string, bool, numbers) carry no payload.
}

type SumType struct {
	Variants []SumVariant `bsatn:"variants"`
}

type SumVariant struct {
	Name *string       `bsatn:"name"`
	Type AlgebraicType `bsatn:"algebraic_type"`
}

type ProductType struct {
	Elements []ProductElement `bsatn:"elements"`
}

type ProductElement struct {
	Name *string       `bsatn:"name"`
	Type AlgebraicType `bsatn:"algebraic_type"`
}

type ArrayType struct {
	Elem AlgebraicType `bsatn:"elem_ty"`
}

// Constructors -------------------------------------------------------------

func RefType(index uint32) AlgebraicType {
	return AlgebraicType{Kind: atRef, Ref: index}
}
func SumTypeOf(variants ...SumVariant) AlgebraicType {
	st := &SumType{Variants: variants}
	return AlgebraicType{Kind: atSum, Sum: st}
}
func ProductTypeOf(elems ...ProductElement) AlgebraicType {
	pt := &ProductType{Elements: elems}
	return AlgebraicType{Kind: atProduct, Product: pt}
}
func ArrayTypeOf(elem AlgebraicType) AlgebraicType {
	return AlgebraicType{Kind: atArray, Array: &ArrayType{Elem: elem}}
}

// Unit helpers -------------------------------------------------------------

func StringType() AlgebraicType { return AlgebraicType{Kind: atString} }
func BoolType() AlgebraicType   { return AlgebraicType{Kind: atBool} }
func I8Type() AlgebraicType     { return AlgebraicType{Kind: atI8} }
func U8Type() AlgebraicType     { return AlgebraicType{Kind: atU8} }
func I16Type() AlgebraicType    { return AlgebraicType{Kind: atI16} }
func U16Type() AlgebraicType    { return AlgebraicType{Kind: atU16} }
func I32Type() AlgebraicType    { return AlgebraicType{Kind: atI32} }
func U32Type() AlgebraicType    { return AlgebraicType{Kind: atU32} }
func I64Type() AlgebraicType    { return AlgebraicType{Kind: atI64} }
func U64Type() AlgebraicType    { return AlgebraicType{Kind: atU64} }
func F32Type() AlgebraicType    { return AlgebraicType{Kind: atF32} }
func F64Type() AlgebraicType    { return AlgebraicType{Kind: atF64} }

// Marshal / Unmarshal -------------------------------------------------------

// MarshalAlgebraicType encodes the type description into BSATN bytes.
// It uses the existing Variant helper so we reuse the generic encoder.
func MarshalAlgebraicType(at AlgebraicType) ([]byte, error) {
	var payload interface{}
	switch at.Kind {
	case atRef:
		payload = at.Ref
	case atSum:
		if at.Sum == nil {
			return nil, ErrInvalidTag
		}
		payload = *at.Sum
	case atProduct:
		if at.Product == nil {
			return nil, ErrInvalidTag
		}
		payload = *at.Product
	case atArray:
		if at.Array == nil {
			return nil, ErrInvalidTag
		}
		payload = *at.Array
	// unit variants – no payload (payload stays nil)
	case atString, atBool, atI8, atU8, atI16, atU16, atI32, atU32, atI64, atU64, atI128, atU128, atI256, atU256, atF32, atF64:
		// nothing
	default:
		return nil, ErrInvalidTag
	}

	v := Variant{Index: uint32(at.Kind)}
	if payload != nil {
		v.Value = payload
	}
	return Marshal(v)
}

// UnmarshalAlgebraicType decodes BSATN bytes back into an AlgebraicType.
func UnmarshalAlgebraicType(buf []byte) (AlgebraicType, error) {
	val, err := Unmarshal(buf)
	if err != nil {
		return AlgebraicType{}, err
	}
	variant, ok := val.(Variant)
	if !ok {
		return AlgebraicType{}, ErrInvalidTag
	}
	kind := atKind(variant.Index)
	var at AlgebraicType
	at.Kind = kind

	switch kind {
	case atRef:
		if variant.Value == nil {
			return AlgebraicType{}, ErrInvalidTag
		}
		if num, ok := variant.Value.(uint32); ok {
			at.Ref = num
		} else {
			return AlgebraicType{}, ErrInvalidTag
		}
	case atSum:
		st, err := decodeSumType(variant.Value)
		if err != nil {
			return AlgebraicType{}, err
		}
		at.Sum = &st
	case atProduct:
		pt, err := decodeProductType(variant.Value)
		if err != nil {
			return AlgebraicType{}, err
		}
		at.Product = &pt
	case atArray:
		arr, err := decodeArrayType(variant.Value)
		if err != nil {
			return AlgebraicType{}, err
		}
		at.Array = &arr
	// unit variants – nothing to pull out
	case atString, atBool, atI8, atU8, atI16, atU16, atI32, atU32, atI64, atU64, atI128, atU128, atI256, atU256, atF32, atF64:
		// ok
	default:
		return AlgebraicType{}, ErrInvalidTag
	}
	return at, nil
}

func decodeSumType(v interface{}) (SumType, error) {
	m, ok := v.(map[string]interface{})
	if !ok {
		return SumType{}, ErrInvalidTag
	}
	raw, ok := m["variants"]
	if !ok {
		return SumType{}, ErrInvalidTag
	}
	slice, ok := raw.([]interface{})
	if !ok {
		return SumType{}, ErrInvalidTag
	}
	variants := make([]SumVariant, 0, len(slice))
	for _, item := range slice {
		elem, ok := item.(map[string]interface{})
		if !ok {
			return SumType{}, ErrInvalidTag
		}
		// name field is an option
		var namePtr *string
		if nRaw, ok := elem["name"]; ok {
			switch v := nRaw.(type) {
			case nil:
				// none
			case string:
				namePtr = new(string)
				*namePtr = v
			case *interface{}:
				if v != nil {
					if s, ok := (*v).(string); ok {
						namePtr = new(string)
						*namePtr = s
					}
				}
			case Variant:
				if v.Index == uint32(TagOptionSome) {
					if s, ok := v.Value.(string); ok {
						namePtr = new(string)
						*namePtr = s
					}
				}
			}
		}
		atRaw, ok := elem["algebraic_type"]
		if !ok {
			return SumType{}, ErrInvalidTag
		}
		at, err := parseAnyToAlgebraicType(atRaw)
		if err != nil {
			return SumType{}, err
		}
		variants = append(variants, SumVariant{Name: namePtr, Type: at})
	}
	return SumType{Variants: variants}, nil
}

func decodeProductType(v interface{}) (ProductType, error) {
	m, ok := v.(map[string]interface{})
	if !ok {
		return ProductType{}, ErrInvalidTag
	}
	raw, ok := m["elements"]
	if !ok {
		return ProductType{}, ErrInvalidTag
	}
	slice, ok := raw.([]interface{})
	if !ok {
		return ProductType{}, ErrInvalidTag
	}
	elements := make([]ProductElement, 0, len(slice))
	for _, item := range slice {
		elem, ok := item.(map[string]interface{})
		if !ok {
			return ProductType{}, ErrInvalidTag
		}
		// name option
		var namePtr *string
		if nRaw, ok := elem["name"]; ok {
			switch v := nRaw.(type) {
			case nil:
			case string:
				namePtr = new(string)
				*namePtr = v
			case *interface{}:
				if v != nil {
					if s, ok := (*v).(string); ok {
						namePtr = new(string)
						*namePtr = s
					}
				}
			case Variant:
				if v.Index == uint32(TagOptionSome) {
					if s, ok := v.Value.(string); ok {
						namePtr = new(string)
						*namePtr = s
					}
				}
			}
		}
		atRaw, ok := elem["algebraic_type"]
		if !ok {
			return ProductType{}, ErrInvalidTag
		}
		at, err := parseAnyToAlgebraicType(atRaw)
		if err != nil {
			return ProductType{}, err
		}
		elements = append(elements, ProductElement{Name: namePtr, Type: at})
	}
	return ProductType{Elements: elements}, nil
}

func decodeArrayType(v interface{}) (ArrayType, error) {
	m, ok := v.(map[string]interface{})
	if !ok {
		return ArrayType{}, ErrInvalidTag
	}
	raw, ok := m["elem_ty"]
	if !ok {
		return ArrayType{}, ErrInvalidTag
	}
	at, err := parseAnyToAlgebraicType(raw)
	if err != nil {
		return ArrayType{}, err
	}
	return ArrayType{Elem: at}, nil
}

// parseAnyToAlgebraicType attempts to convert an interface{} that is the result
// of bsatn.Unmarshal back into an AlgebraicType value.
func parseAnyToAlgebraicType(val interface{}) (AlgebraicType, error) {
	switch x := val.(type) {
	case Variant:
		// fast path: already variant
		buf, err := Marshal(x)
		if err != nil {
			return AlgebraicType{}, err
		}
		return UnmarshalAlgebraicType(buf)
	case map[string]interface{}:
		// struct representation – look for known keys
		if _, ok := x["ref"]; ok {
			if num, ok2 := x["ref"].(uint32); ok2 {
				return RefType(num), nil
			}
		}
		if inner, ok := x["sum"]; ok {
			st, err := decodeSumType(inner)
			if err != nil {
				return AlgebraicType{}, err
			}
			return AlgebraicType{Kind: atSum, Sum: &st}, nil
		}
		if inner, ok := x["product"]; ok {
			pt, err := decodeProductType(inner)
			if err != nil {
				return AlgebraicType{}, err
			}
			return AlgebraicType{Kind: atProduct, Product: &pt}, nil
		}
		if inner, ok := x["array"]; ok {
			arr, err := decodeArrayType(inner)
			if err != nil {
				return AlgebraicType{}, err
			}
			return AlgebraicType{Kind: atArray, Array: &arr}, nil
		}
		// heuristic: if map has key "elements" treat as embedded ProductType
		if _, hasElems := x["elements"]; hasElems {
			pt, err := decodeProductType(x)
			if err != nil {
				return AlgebraicType{}, err
			}
			return AlgebraicType{Kind: atProduct, Product: &pt}, nil
		}
		if _, hasVars := x["variants"]; hasVars {
			st, err := decodeSumType(x)
			if err != nil {
				return AlgebraicType{}, err
			}
			return AlgebraicType{Kind: atSum, Sum: &st}, nil
		}
		if _, hasElemTy := x["elem_ty"]; hasElemTy {
			arr, err := decodeArrayType(x)
			if err != nil {
				return AlgebraicType{}, err
			}
			return AlgebraicType{Kind: atArray, Array: &arr}, nil
		}
		// unit variants encoded as empty struct -> determine via single key with empty struct maybe string
		// Fallback invalid
		return AlgebraicType{}, ErrInvalidTag
	default:
		return AlgebraicType{}, ErrInvalidTag
	}
}
