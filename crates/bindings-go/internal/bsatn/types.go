package bsatn

import (
	"log"
	"strings"
)

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
	atBytes   // For []byte
	atOption  // For optional values
	atOpaque  // For opaque types handled by custom serializers
	atUnknown // Placeholder for unknown or error states if needed during parsing
)

// Int128 represents a 128-bit signed integer.
// Internally, it can be represented as [16]byte or [2]uint64 for easier manipulation.
// For BSATN, it's often sent as a 16-byte array.
type Int128 struct {
	Bytes [16]byte // Big-endian representation
}

// Uint128 represents a 128-bit unsigned integer.
type Uint128 struct {
	Bytes [16]byte // Big-endian representation
}

// AlgebraicType is the union.
type AlgebraicType struct {
	Kind    atKind         `bsatn:"-"`
	Ref     uint32         `bsatn:"ref,omitempty"`
	Sum     *SumType       `bsatn:"sum,omitempty"`
	Product *ProductType   `bsatn:"product,omitempty"`
	Array   *ArrayType     `bsatn:"array,omitempty"`
	Option  *AlgebraicType `bsatn:"option,omitempty"`
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
func OptionTypeOf(innerType AlgebraicType) AlgebraicType {
	return AlgebraicType{Kind: atOption, Option: &innerType}
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

// Constructors for 128-bit type schemas
func I128Type() AlgebraicType { return AlgebraicType{Kind: atI128} }
func U128Type() AlgebraicType { return AlgebraicType{Kind: atU128} }

// Marshal / Unmarshal -------------------------------------------------------

// MarshalAlgebraicType encodes the type description into BSATN bytes using a Writer.
func MarshalAlgebraicType(w *Writer, at AlgebraicType) {
	if w.Error() != nil {
		return
	}
	var payload interface{}
	switch at.Kind {
	case atRef:
		payload = at.Ref
	case atSum:
		if at.Sum == nil {
			w.recordError(ErrInvalidTag)
			return
		}
		payload = *at.Sum
	case atProduct:
		if at.Product == nil {
			w.recordError(ErrInvalidTag)
			return
		}
		payload = *at.Product
	case atArray:
		if at.Array == nil {
			w.recordError(ErrInvalidTag)
			return
		}
		payload = *at.Array
	case atOption:
		if at.Option == nil {
			w.recordError(ErrInvalidTag)
			return
		}
		payload = *at.Option
	case atString, atBool, atI8, atU8, atI16, atU16, atI32, atU32, atI64, atU64, atI128, atU128, atI256, atU256, atF32, atF64:
		// nothing, payload remains nil for unit variants
	default:
		w.recordError(ErrInvalidTag)
		return
	}

	w.WriteEnumHeader(uint32(at.Kind))
	if payload != nil {
		marshalRecursive(w, payload)
	}
}

// UnmarshalAlgebraicType decodes BSATN bytes back into an AlgebraicType.
func UnmarshalAlgebraicType(buf []byte) (AlgebraicType, error) {
	log.Printf("[UnmarshalAlgebraicType] entry, len(buf)=%d", len(buf))
	val, _, err := Unmarshal(buf)
	log.Printf("[UnmarshalAlgebraicType] Unmarshal returned: val=%#v, err=%v", val, err)
	if err != nil {
		return AlgebraicType{}, err
	}
	variant, ok := val.(Variant)
	log.Printf("[UnmarshalAlgebraicType] Variant assertion: ok=%t, variant=%#v", ok, variant)
	if !ok {
		log.Printf("[UnmarshalAlgebraicType] error: val was not a Variant, type was %T", val)
		return AlgebraicType{}, ErrInvalidTag
	}
	kind := atKind(variant.Index)
	log.Printf("[UnmarshalAlgebraicType] kind=%d (%s)", kind, kind.String())
	var at AlgebraicType
	at.Kind = kind

	switch kind {
	case atRef:
		log.Printf("[UnmarshalAlgebraicType] kind=atRef, variant.Value=%#v", variant.Value)
		if variant.Value == nil {
			log.Printf("[UnmarshalAlgebraicType atRef] error: variant.Value is nil")
			return AlgebraicType{}, ErrInvalidTag
		}
		if num, ok := variant.Value.(uint32); ok {
			at.Ref = num
		} else {
			log.Printf("[UnmarshalAlgebraicType atRef] error: variant.Value is not uint32, type was %T", variant.Value)
			return AlgebraicType{}, ErrInvalidTag
		}
	case atSum:
		log.Printf("[UnmarshalAlgebraicType] kind=atSum, decoding SumType from variant.Value=%#v", variant.Value)
		st, err := decodeSumType(variant.Value)
		if err != nil {
			log.Printf("[UnmarshalAlgebraicType atSum] error from decodeSumType: %v", err)
			return AlgebraicType{}, err
		}
		at.Sum = &st
	case atProduct:
		log.Printf("[UnmarshalAlgebraicType] kind=atProduct, decoding ProductType from variant.Value=%#v", variant.Value)
		innerMap, okMap := variant.Value.(map[string]interface{})
		if !okMap {
			log.Printf("[UnmarshalAlgebraicType atProduct] error: variant.Value is not map[string]interface{}, type was %T", variant.Value)
			return AlgebraicType{}, ErrInvalidTag
		}
		pt, err := decodeProductType(innerMap)
		if err != nil {
			log.Printf("[UnmarshalAlgebraicType atProduct] error from decodeProductType: %v", err)
			return AlgebraicType{}, err
		}
		at.Product = &pt
	case atArray:
		log.Printf("[UnmarshalAlgebraicType] kind=atArray, decoding ArrayType from variant.Value=%#v", variant.Value)
		innerMap, okMap := variant.Value.(map[string]interface{})
		if !okMap {
			log.Printf("[UnmarshalAlgebraicType atArray] error: variant.Value is not map[string]interface{}, type was %T", variant.Value)
			return AlgebraicType{}, ErrInvalidTag
		}
		arr, err := decodeArrayType(innerMap)
		if err != nil {
			log.Printf("[UnmarshalAlgebraicType atArray] error from decodeArrayType: %v", err)
			return AlgebraicType{}, err
		}
		at.Array = &arr
	case atOption:
		log.Printf("[UnmarshalAlgebraicType] kind=atOption, decoding Option inner type from variant.Value=%#v", variant.Value)
		if variant.Value == nil {
			log.Printf("[UnmarshalAlgebraicType atOption] error: variant.Value is nil for Option schema, expected inner type schema")
			return AlgebraicType{}, ErrInvalidTag
		}
		innerAt, err := parseAnyToAlgebraicType(variant.Value)
		if err != nil {
			log.Printf("[UnmarshalAlgebraicType atOption] error from parseAnyToAlgebraicType for inner type: %v", err)
			return AlgebraicType{}, err
		}
		at.Option = &innerAt
	case atString, atBool, atI8, atU8, atI16, atU16, atI32, atU32, atI64, atU64, atI128, atU128, atI256, atU256, atF32, atF64:
		log.Printf("[UnmarshalAlgebraicType] kind=%s is a unit variant", kind.String())
	default:
		log.Printf("[UnmarshalAlgebraicType] error: unknown kind %d", kind)
		return AlgebraicType{}, ErrInvalidTag
	}
	log.Printf("[UnmarshalAlgebraicType] successfully unmarshaled: %#v", at)
	return at, nil
}

func decodeSumType(v interface{}) (SumType, error) {
	log.Printf("[decodeSumType] entry, v=%#v", v)
	m, ok := v.(map[string]interface{})
	log.Printf("[decodeSumType] asserted v to map: ok=%t, m=%#v", ok, m)
	if !ok {
		log.Printf("[decodeSumType] error: input v is not map[string]interface{}, type was %T", v)
		return SumType{}, ErrInvalidTag
	}
	raw, ok := m["variants"]
	log.Printf("[decodeSumType] m[\"variants\"]: ok=%t, raw=%#v", ok, raw)
	if !ok {
		log.Printf("[decodeSumType] error: key \"variants\" not found in map")
		return SumType{}, ErrInvalidTag
	}
	slice, ok := raw.([]interface{})
	log.Printf("[decodeSumType] asserted raw to slice: ok=%t, slice len=%d", ok, len(slice))
	if !ok {
		log.Printf("[decodeSumType] error: raw for \"variants\" is not []interface{}, type was %T", raw)
		return SumType{}, ErrInvalidTag
	}
	variants := make([]SumVariant, 0, len(slice))
	for i, item := range slice {
		log.Printf("[decodeSumType] item %d: %#v", i, item)
		elem, ok := item.(map[string]interface{})
		log.Printf("[decodeSumType] item %d asserted to map: ok=%t, elem=%#v", i, ok, elem)
		if !ok {
			log.Printf("[decodeSumType] error item %d: not map[string]interface{}, type was %T", i, item)
			return SumType{}, ErrInvalidTag
		}
		var namePtr *string
		if nRaw, okName := elem["name"]; okName {
			log.Printf("[decodeSumType] item %d, name field raw: %#v", i, nRaw)
			switch vn := nRaw.(type) {
			case nil:
				log.Printf("[decodeSumType] item %d, name is nil (OptionNone)", i)
			case string:
				namePtr = new(string)
				*namePtr = vn
				log.Printf("[decodeSumType] item %d, name is string: %s", i, *namePtr)
			case *interface{}:
				if vn != nil {
					if s, okStr := (*vn).(string); okStr {
						namePtr = new(string)
						*namePtr = s
						log.Printf("[decodeSumType] item %d, name is *interface{}->string: %s", i, *namePtr)
					} else {
						log.Printf("[decodeSumType] item %d, name is *interface{} but not string, type: %T", i, *vn)
					}
				} else {
					log.Printf("[decodeSumType] item %d, name is nil *interface{}", i)
				}
			case Variant:
				log.Printf("[decodeSumType] item %d, name is Variant: %#v", i, vn)
				if vn.Index == uint32(TagOptionSome) {
					if s, oks := vn.Value.(string); oks {
						namePtr = &s
						log.Printf("[decodeSumType] item %d, name Variant(Some) string: %s", i, *namePtr)
					} else {
						log.Printf("[decodeSumType] item %d, name Variant(Some) not string: %T", i, vn.Value)
					}
				} else if vn.Index == uint32(TagOptionNone) {
					namePtr = nil
					log.Printf("[decodeSumType] item %d, name Variant(None)", i)
				}
			default:
				log.Printf("[decodeSumType] item %d, name has unhandled type: %T, value: %#v", i, vn, vn)
			}
		} else {
			log.Printf("[decodeSumType] item %d, no 'name' field found", i)
		}
		atRaw, okAt := elem["algebraic_type"]
		log.Printf("[decodeSumType] item %d, algebraic_type field raw: okAt=%t, atRaw=%#v", i, okAt, atRaw)
		if !okAt {
			log.Printf("[decodeSumType] error item %d: key \"algebraic_type\" not found", i)
			return SumType{}, ErrInvalidTag
		}
		at, err := parseAnyToAlgebraicType(atRaw)
		log.Printf("[decodeSumType] item %d, parseAnyToAlgebraicType for atRaw returned: at=%#v, err=%v", i, at, err)
		if err != nil {
			return SumType{}, err
		}
		variants = append(variants, SumVariant{Name: namePtr, Type: at})
	}
	log.Printf("[decodeSumType] successfully decoded SumType: %#v", variants)
	return SumType{Variants: variants}, nil
}

func decodeProductType(v interface{}) (ProductType, error) {
	log.Printf("[decodeProductType] entry, v=%#v", v)
	m, ok := v.(map[string]interface{})
	log.Printf("[decodeProductType] asserted v to map: ok=%t, m=%#v", ok, m)
	if !ok {
		log.Printf("[decodeProductType] error: input v is not map[string]interface{}, type was %T", v)
		return ProductType{}, ErrInvalidTag
	}
	var slice []interface{}
	if raw, okElements := m["elements"]; okElements {
		log.Printf("[decodeProductType] m[\"elements\"]: raw=%#v", raw)
		if raw != nil {
			var ok2 bool
			slice, ok2 = raw.([]interface{})
			log.Printf("[decodeProductType] asserted raw to slice: ok2=%t, slice len=%d", ok2, len(slice))
			if !ok2 {
				log.Printf("[decodeProductType] error: raw for \"elements\" is not []interface{}, type was %T", raw)
				return ProductType{}, ErrInvalidTag
			}
		}
	} else {
		log.Printf("[decodeProductType] key \"elements\" not found in map, treating as empty product")
	}

	if slice == nil {
		log.Printf("[decodeProductType] slice is nil, initializing to empty []interface{}")
		slice = []interface{}{}
	}
	elements := make([]ProductElement, 0, len(slice))
	for i, item := range slice {
		log.Printf("[decodeProductType] item %d: %#v", i, item)
		elem, ok := item.(map[string]interface{})
		log.Printf("[decodeProductType] item %d asserted to map: ok=%t, elem=%#v", i, ok, elem)
		if !ok {
			log.Printf("[decodeProductType] error item %d: not map[string]interface{}, type was %T", i, item)
			return ProductType{}, ErrInvalidTag
		}
		var namePtr *string
		if nRaw, okName := elem["name"]; okName {
			log.Printf("[decodeProductType] item %d, name field raw: %#v", i, nRaw)
			switch vn := nRaw.(type) {
			case nil:
				namePtr = nil
				log.Printf("[decodeProductType] item %d, name is nil (OptionNone)", i)
			case string:
				namePtr = &vn
				log.Printf("[decodeProductType] item %d, name is string: %s", i, *namePtr)
			case *interface{}:
				if vn != nil {
					if s, oks := (*vn).(string); oks {
						namePtr = &s
						log.Printf("[decodeProductType] item %d, name is *interface{}->string: %s", i, *namePtr)
					} else {
						log.Printf("[decodeProductType] item %d, name *interface{} not string: %T", i, *vn)
					}
				} else {
					log.Printf("[decodeProductType] item %d, name is nil *interface{}", i)
				}
			case Variant:
				log.Printf("[decodeProductType] item %d, name is Variant: %#v", i, vn)
				if vn.Index == uint32(TagOptionSome) {
					if s, oks := vn.Value.(string); oks {
						namePtr = &s
						log.Printf("[decodeProductType] item %d, name Variant(Some) string: %s", i, *namePtr)
					} else {
						log.Printf("[decodeProductType] item %d, name Variant(Some) not string: %T", i, vn.Value)
					}
				} else if vn.Index == uint32(TagOptionNone) {
					namePtr = nil
					log.Printf("[decodeProductType] item %d, name Variant(None)", i)
				}
			default:
				log.Printf("[decodeProductType] item %d, name unhandled type: %T val: %#v", i, vn, vn)
			}
		} else {
			log.Printf("[decodeProductType] item %d, no 'name' field", i)
		}

		atRaw, okAt := elem["algebraic_type"]
		log.Printf("[decodeProductType] item %d, algebraic_type field raw: okAt=%t, atRaw=%#v", i, okAt, atRaw)
		if !okAt {
			log.Printf("[decodeProductType] error item %d: key \"algebraic_type\" not found", i)
			return ProductType{}, ErrInvalidTag
		}
		at, err := parseAnyToAlgebraicType(atRaw)
		log.Printf("[decodeProductType] item %d, parseAnyToAlgebraicType for atRaw returned: at=%#v, err=%v", i, at, err)
		if err != nil {
			return ProductType{}, err
		}
		elements = append(elements, ProductElement{Name: namePtr, Type: at})
	}
	log.Printf("[decodeProductType] successfully decoded ProductType: %#v", elements)
	return ProductType{Elements: elements}, nil
}

func decodeArrayType(v interface{}) (ArrayType, error) {
	log.Printf("[decodeArrayType] entry, v=%#v", v)
	m, ok := v.(map[string]interface{})
	log.Printf("[decodeArrayType] asserted v to map: ok=%t, m=%#v", ok, m)
	if !ok {
		log.Printf("[decodeArrayType] error: input v is not map[string]interface{}, type was %T", v)
		return ArrayType{}, ErrInvalidTag
	}
	raw, ok := m["elem_ty"]
	log.Printf("[decodeArrayType] m[\"elem_ty\"]: ok=%t, raw=%#v", ok, raw)
	if !ok {
		log.Printf("[decodeArrayType] error: key \"elem_ty\" not found")
		return ArrayType{}, ErrInvalidTag
	}
	log.Printf("[decodeArrayType] calling parseAnyToAlgebraicType with atRaw=%#v", raw)
	at, err := parseAnyToAlgebraicType(raw)
	log.Printf("[decodeArrayType] parseAnyToAlgebraicType for raw returned: at=%#v, err=%v", at, err)
	if err != nil {
		return ArrayType{}, err
	}
	log.Printf("[decodeArrayType] successfully decoded ArrayType: %#v", at)
	return ArrayType{Elem: at}, nil
}

// parseAnyToAlgebraicType attempts to convert an interface{} that is the result
// of bsatn.Unmarshal back into an AlgebraicType value.
var parseDepth = 0

func parseAnyToAlgebraicType(val interface{}) (AlgebraicType, error) {
	parseDepth++
	prefix := strings.Repeat("  ", parseDepth)
	log.Printf("%s[parseAnyToAlgebraicType depth %d] entry, val=%#v, type=%T", prefix, parseDepth, val, val)
	var result AlgebraicType
	var err error

	switch x := val.(type) {
	case Variant:
		log.Printf("%s[parseAnyToAlgebraicType depth %d] case Variant: %#v", prefix, parseDepth, x)
		var buf []byte
		buf, err = Marshal(x)
		if err != nil {
			log.Printf("%s[parseAnyToAlgebraicType depth %d] Marshal(Variant) failed: %v", prefix, parseDepth, err)
		} else {
			log.Printf("%s[parseAnyToAlgebraicType depth %d] recursively calling UnmarshalAlgebraicType with marshaled variant (len %d)", prefix, parseDepth, len(buf))
			result, err = UnmarshalAlgebraicType(buf)
		}
	case map[string]interface{}:
		log.Printf("%s[parseAnyToAlgebraicType depth %d] case map[string]interface{}: %#v", prefix, parseDepth, x)
		if refVal, okRef := x["ref"]; okRef {
			if num, okNum := refVal.(uint32); okNum {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'ref': %d", prefix, parseDepth, num)
				result = RefType(num)
			} else {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'ref' but not uint32: %T", prefix, parseDepth, refVal)
				err = ErrInvalidTag
			}
		} else if sumVal, okSum := x["sum"]; okSum {
			if innerMap, okMap := sumVal.(map[string]interface{}); okMap {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'sum', decoding SumType from %#v", prefix, parseDepth, innerMap)
				var st SumType
				st, err = decodeSumType(innerMap)
				if err == nil {
					result = AlgebraicType{Kind: atSum, Sum: &st}
				}
			} else {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'sum' but its value is not map[string]interface{}, type was %T", prefix, parseDepth, sumVal)
				err = ErrInvalidTag
			}
		} else if prodVal, okProd := x["product"]; okProd {
			if innerMap, okMap := prodVal.(map[string]interface{}); okMap {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'product', decoding ProductType from %#v", prefix, parseDepth, innerMap)
				var pt ProductType
				pt, err = decodeProductType(innerMap)
				if err == nil {
					result = AlgebraicType{Kind: atProduct, Product: &pt}
				}
			} else {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'product' but its value is not map[string]interface{}, type was %T", prefix, parseDepth, prodVal)
				err = ErrInvalidTag
			}
		} else if arrayVal, okArray := x["array"]; okArray {
			if innerMap, okMap := arrayVal.(map[string]interface{}); !okMap {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'array' but its value is not map[string]interface{}, type was %T", prefix, parseDepth, arrayVal)
				err = ErrInvalidTag
			} else {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'array', decoding ArrayType from %#v", prefix, parseDepth, innerMap)
				var arr ArrayType
				arr, err = decodeArrayType(innerMap)
				if err == nil {
					result = AlgebraicType{Kind: atArray, Array: &arr}
				}
			}
		} else if _, hasElems := x["elements"]; hasElems {
			log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'elements', treating as embedded ProductType from %#v", prefix, parseDepth, x)
			var pt ProductType
			pt, err = decodeProductType(x)
			if err == nil {
				result = AlgebraicType{Kind: atProduct, Product: &pt}
			}
		} else if _, hasVars := x["variants"]; hasVars {
			log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'variants', treating as embedded SumType from %#v", prefix, parseDepth, x)
			var st SumType
			st, err = decodeSumType(x)
			if err == nil {
				result = AlgebraicType{Kind: atSum, Sum: &st}
			}
		} else if _, hasElemTy := x["elem_ty"]; hasElemTy {
			log.Printf("%s[parseAnyToAlgebraicType depth %d] map has 'elem_ty', treating as embedded ArrayType from %#v", prefix, parseDepth, x)
			var arr ArrayType
			arr, err = decodeArrayType(x)
			if err == nil {
				result = AlgebraicType{Kind: atArray, Array: &arr}
			}
		} else {
			foundUnitKey := false
			for k, mapKind := range unitVariantMapKeys {
				if _, okUnit := x[k]; okUnit {
					log.Printf("%s[parseAnyToAlgebraicType depth %d] map has unit key '%s', kind %s", prefix, parseDepth, k, mapKind.String())
					result = AlgebraicType{Kind: mapKind}
					foundUnitKey = true
					break
				}
			}
			if !foundUnitKey {
				log.Printf("%s[parseAnyToAlgebraicType depth %d] map has no known keys for AlgebraicType, fallback invalid tag", prefix, parseDepth)
				err = ErrInvalidTag
			}
		}
	case uint32:
		log.Printf("%s[parseAnyToAlgebraicType depth %d] case uint32: %d", prefix, parseDepth, x)
		switch atKind(x) {
		case atString:
			result = StringType()
		case atBool:
			result = BoolType()
		case atI8:
			result = I8Type()
		case atU8:
			result = U8Type()
		case atI16:
			result = I16Type()
		case atU16:
			result = U16Type()
		case atI32:
			result = I32Type()
		case atU32:
			result = U32Type()
		case atI64:
			result = I64Type()
		case atU64:
			result = U64Type()
		case atF32:
			result = F32Type()
		case atF64:
			result = F64Type()
		default:
			log.Printf("%s[parseAnyToAlgebraicType depth %d] uint32 value %d is not a known unit primitive kind", prefix, parseDepth, x)
			err = ErrInvalidTag
		}
	default:
		log.Printf("%s[parseAnyToAlgebraicType depth %d] unhandled type %T, fallback invalid tag", prefix, parseDepth, x)
		err = ErrInvalidTag
	}

	if err != nil {
		log.Printf("%s[parseAnyToAlgebraicType depth %d] returning error: %v. Input val: %#v", prefix, parseDepth, err, val)
		parseDepth--
		return AlgebraicType{}, err
	}
	log.Printf("%s[parseAnyToAlgebraicType depth %d] successfully parsed: %#v", prefix, parseDepth, result)
	parseDepth--
	return result, nil
}

// Helper for parseAnyToAlgebraicType to map string keys to unit kinds (if coming from JSON-like map)
var unitVariantMapKeys = map[string]atKind{
	"string": atString,
	"bool":   atBool,
	"i8":     atI8,
	"u8":     atU8,
	"i16":    atI16,
	"u16":    atU16,
	"i32":    atI32,
	"u32":    atU32,
	"i64":    atI64,
	"u64":    atU64,
	"f32":    atF32,
	"f64":    atF64,
}

// String method for atKind for better logging
func (ak atKind) String() string {
	switch ak {
	case atRef:
		return "atRef"
	case atSum:
		return "atSum"
	case atProduct:
		return "atProduct"
	case atArray:
		return "atArray"
	case atString:
		return "atString"
	case atBool:
		return "atBool"
	case atI8:
		return "atI8"
	case atU8:
		return "atU8"
	case atI16:
		return "atI16"
	case atU16:
		return "atU16"
	case atI32:
		return "atI32"
	case atU32:
		return "atU32"
	case atI64:
		return "atI64"
	case atU64:
		return "atU64"
	case atI128:
		return "atI128"
	case atU128:
		return "atU128"
	case atI256:
		return "atI256"
	case atU256:
		return "atU256"
	case atF32:
		return "atF32"
	case atF64:
		return "atF64"
	case atBytes:
		return "atBytes"
	case atOption:
		return "atOption"
	case atOpaque:
		return "atOpaque"
	case atUnknown:
		return "atUnknown"
	default:
		return "unknown_atKind"
	}
}
