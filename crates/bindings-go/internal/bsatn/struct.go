package bsatn

import (
	"reflect"
)

// UnmarshalInto decodes a BSATN buffer that contains a struct payload into the given pointer to struct.
// Only exported fields are set. Tag `bsatn:"name"` is honoured; `bsatn:"-"` skips the field.
func UnmarshalInto(buf []byte, out interface{}) error {
	if reflect.TypeOf(out).Kind() != reflect.Ptr {
		return ErrInvalidTag // misuse
	}
	vptr := reflect.ValueOf(out)
	if vptr.IsNil() {
		return ErrInvalidTag
	}
	v := vptr.Elem()
	if v.Kind() != reflect.Struct {
		return ErrInvalidTag
	}

	val, err := Unmarshal(buf)
	if err != nil {
		return err
	}
	m, ok := val.(map[string]interface{})
	if !ok {
		return ErrInvalidTag
	}

	t := v.Type()
	for i := 0; i < v.NumField(); i++ {
		sf := t.Field(i)
		if sf.PkgPath != "" { // unexported
			continue
		}
		tagName := sf.Tag.Get("bsatn")
		if tagName == "-" {
			continue
		}
		name := sf.Name
		if tagName != "" {
			name = tagName
		}
		if mv, exists := m[name]; exists {
			fv := v.Field(i)
			if fv.CanSet() {
				// simple set for primitives compatible by kind
				switch fv.Kind() {
				case reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uint:
					fv.SetUint(reflect.ValueOf(mv).Convert(fv.Type()).Uint())
				case reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64, reflect.Int:
					fv.SetInt(reflect.ValueOf(mv).Convert(fv.Type()).Int())
				case reflect.Float32, reflect.Float64:
					fv.SetFloat(reflect.ValueOf(mv).Convert(fv.Type()).Float())
				case reflect.String:
					if s, ok := mv.(string); ok {
						fv.SetString(s)
					}
				case reflect.Bool:
					if b, ok := mv.(bool); ok {
						fv.SetBool(b)
					}
				}
			}
		}
	}
	return nil
}
