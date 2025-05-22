package bsatn

import (
	"bytes"
	"fmt"
	"math"
	"reflect"
	// "log" // Already in other files, keep if specific UnmarshalInto logs are added
)

// UnmarshalInto decodes a BSATN buffer that contains a struct payload into the given pointer to struct.
// Only exported fields are set. Tag `bsatn:"name"` is honoured; `bsatn:"-"` skips the field.
// If the target struct implements IStructuralReadWrite, its ReadBSATN method is used.
func UnmarshalInto(buf []byte, out interface{}) error {
	targetType := reflect.TypeOf(out)
	if targetType.Kind() != reflect.Ptr {
		return fmt.Errorf("bsatn: UnmarshalInto target must be a pointer, got %T", out)
	}
	vptr := reflect.ValueOf(out)
	if vptr.IsNil() {
		return fmt.Errorf("bsatn: UnmarshalInto target cannot be a nil pointer")
	}
	v := vptr.Elem() // The actual struct value
	if v.Kind() != reflect.Struct {
		return fmt.Errorf("bsatn: UnmarshalInto target must be a pointer to a struct, got pointer to %s", v.Kind())
	}

	bytesReader := bytes.NewReader(buf)
	r := NewReader(bytesReader)

	// Check if the type implements IStructuralReadWrite
	if custom, ok := out.(IStructuralReadWrite); ok {
		err := custom.ReadBSATN(r)
		if err != nil {
			return err // Error from custom ReadBSATN method
		}
		return r.Error() // Check for any errors accumulated in the reader by the custom method
	}

	// Default reflection-based unmarshaling if IStructuralReadWrite is not implemented
	tag, err := r.ReadTag()
	if err != nil {
		return fmt.Errorf("bsatn: UnmarshalInto failed to read initial tag: %w", err)
	}
	if tag != TagStruct {
		return fmt.Errorf("bsatn: UnmarshalInto expected TagStruct, got tag 0x%x", tag)
	}

	fieldCount, err := r.ReadStructHeader()
	if err != nil {
		return fmt.Errorf("bsatn: UnmarshalInto failed to read struct header: %w", err)
	}

	// Read all fields into a temporary map first, as BSATN fields are sorted by name during encoding,
	// but Go struct fields are processed by reflection in their declaration order.
	m := make(map[string]interface{}, fieldCount)
	for i := uint32(0); i < fieldCount; i++ {
		fieldName, err := r.ReadFieldName()
		if err != nil {
			return fmt.Errorf("bsatn: UnmarshalInto failed to read field name for field %d: %w", i, err)
		}
		fieldValue := unmarshalRecursive(r) // Use the recursive helper from decode.go
		if r.Error() != nil {
			return fmt.Errorf("bsatn: UnmarshalInto failed to unmarshal value for field '%s': %w", fieldName, r.Error())
		}
		m[fieldName] = fieldValue
	}

	// Now populate the struct fields from the map
	structType := v.Type()
	for i := 0; i < v.NumField(); i++ {
		sf := structType.Field(i)
		if sf.PkgPath != "" { // unexported
			continue
		}
		tagName := sf.Tag.Get("bsatn")
		if tagName == "-" {
			continue
		}
		fieldName := sf.Name
		if tagName != "" {
			fieldName = tagName
		}

		if mv, exists := m[fieldName]; exists {
			fv := v.Field(i)
			if fv.CanSet() {
				sourceValReflection := reflect.ValueOf(mv)
				targetFieldType := fv.Type()

				switch fv.Kind() {
				case reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uint:
					var u64Val uint64
					switch sVal := mv.(type) {
					case uint8:
						u64Val = uint64(sVal)
					case uint16:
						u64Val = uint64(sVal)
					case uint32:
						u64Val = uint64(sVal)
					case uint64:
						u64Val = sVal
					case int8:
						if sVal < 0 {
							return ErrOverflow
						}
						u64Val = uint64(sVal)
					case int16:
						if sVal < 0 {
							return ErrOverflow
						}
						u64Val = uint64(sVal)
					case int32:
						if sVal < 0 {
							return ErrOverflow
						}
						u64Val = uint64(sVal)
					case int64:
						if sVal < 0 {
							return ErrOverflow
						}
						u64Val = uint64(sVal)
					default:
						if sourceValReflection.Kind() == reflect.Interface && sourceValReflection.IsNil() {
							return fmt.Errorf("bsatn: cannot convert nil to %s for field %s", targetFieldType.Name(), fieldName)
						}
						if !sourceValReflection.CanConvert(reflect.TypeOf(u64Val)) {
							return fmt.Errorf("bsatn: cannot convert %T to uint64 for field %s", mv, fieldName)
						}
						u64Val = sourceValReflection.Convert(reflect.TypeOf(u64Val)).Uint()
					}
					targetMax := uint64(1)
					if targetFieldType.Bits() < 64 {
						targetMax = (1 << targetFieldType.Bits()) - 1
					} else {
						targetMax = math.MaxUint64
					}
					if u64Val > targetMax {
						return ErrOverflow
					}
					fv.SetUint(u64Val)
				case reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64, reflect.Int:
					var i64Val int64
					switch sVal := mv.(type) {
					case int8:
						i64Val = int64(sVal)
					case int16:
						i64Val = int64(sVal)
					case int32:
						i64Val = int64(sVal)
					case int64:
						i64Val = sVal
					case uint8:
						i64Val = int64(sVal)
					case uint16:
						i64Val = int64(sVal)
					case uint32:
						i64Val = int64(sVal)
					case uint64:
						if sVal > math.MaxInt64 {
							return ErrOverflow
						}
						i64Val = int64(sVal)
					default:
						if sourceValReflection.Kind() == reflect.Interface && sourceValReflection.IsNil() {
							return fmt.Errorf("bsatn: cannot convert nil to %s for field %s", targetFieldType.Name(), fieldName)
						}
						if !sourceValReflection.CanConvert(reflect.TypeOf(i64Val)) {
							return fmt.Errorf("bsatn: cannot convert %T to int64 for field %s", mv, fieldName)
						}
						i64Val = sourceValReflection.Convert(reflect.TypeOf(i64Val)).Int()
					}

					targetBits := targetFieldType.Bits()
					minTarget := int64(0)
					maxTarget := int64(0)

					if targetBits <= 0 || targetBits > 64 {
						fv.SetInt(i64Val)
					} else {
						minTarget = -(1 << (targetBits - 1))
						maxTarget = (1 << (targetBits - 1)) - 1
						if i64Val < minTarget || i64Val > maxTarget {
							return ErrOverflow
						}
						fv.SetInt(i64Val)
					}
				case reflect.Float32, reflect.Float64:
					// TODO: Add NaN/Inf validation if required by task spec
					fv.SetFloat(sourceValReflection.Convert(targetFieldType).Float())
				case reflect.String:
					if s, okConv := mv.(string); okConv {
						fv.SetString(s)
					} else {
						return fmt.Errorf("bsatn: cannot convert %T to string for field %s", mv, fieldName)
					}
				case reflect.Bool:
					if b, okConv := mv.(bool); okConv {
						fv.SetBool(b)
					} else {
						return fmt.Errorf("bsatn: cannot convert %T to bool for field %s", mv, fieldName)
					}
				case reflect.Slice:
					if decSlice, okConv := mv.([]byte); okConv && targetFieldType.Elem().Kind() == reflect.Uint8 {
						// Handle []byte for U128/U256 etc. if target is []byte
						fv.SetBytes(decSlice)
					} else if targetFieldType.AssignableTo(sourceValReflection.Type()) {
						fv.Set(sourceValReflection)
					} else {
						// Handle other slice types if necessary, e.g. []OtherStruct
						return fmt.Errorf("bsatn: type mismatch or unhandled slice conversion for field %s: cannot convert %T to %s", fieldName, mv, targetFieldType.String())
					}
				case reflect.Array:
					decSlice, okConv := mv.([]byte) // U128/U256 are unmarshaled as []byte
					if okConv && targetFieldType.Elem().Kind() == reflect.Uint8 && fv.Len() == len(decSlice) {
						reflect.Copy(fv, reflect.ValueOf(decSlice)) // Copy to fixed-size array
					} else if targetFieldType.AssignableTo(sourceValReflection.Type()) {
						fv.Set(sourceValReflection)
					} else {
						return fmt.Errorf("bsatn: type mismatch or unhandled array conversion for field %s: cannot convert %T to %s", fieldName, mv, targetFieldType.String())
					}
				default:
					// Attempt direct assignment if types are compatible or convertible by reflection
					if sourceValReflection.Type().AssignableTo(targetFieldType) {
						fv.Set(sourceValReflection)
					} else if sourceValReflection.CanConvert(targetFieldType) {
						fv.Set(sourceValReflection.Convert(targetFieldType))
					} else {
						return fmt.Errorf("bsatn: unhandled type %s for field %s, cannot convert from %T", targetFieldType.Name(), fieldName, mv)
					}
				}
			}
		}
	}
	return r.Error() // Return any error accumulated by the reader during field value unmarshaling
}
