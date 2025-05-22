package bsatn

import "fmt"

// TagToString converts a BSATN tag byte to a human-readable name.
// Useful for diagnostics, pretty-printing and error messages.
func TagToString(tag byte) string {
	switch tag {
	case TagBoolFalse:
		return "TagBoolFalse"
	case TagBoolTrue:
		return "TagBoolTrue"
	case TagU8:
		return "TagU8"
	case TagI8:
		return "TagI8"
	case TagU16:
		return "TagU16"
	case TagI16:
		return "TagI16"
	case TagU32:
		return "TagU32"
	case TagI32:
		return "TagI32"
	case TagU64:
		return "TagU64"
	case TagI64:
		return "TagI64"
	case TagF32:
		return "TagF32"
	case TagF64:
		return "TagF64"
	case TagString:
		return "TagString"
	case TagBytes:
		return "TagBytes"
	case TagList:
		return "TagList"
	case TagOptionNone:
		return "TagOptionNone"
	case TagOptionSome:
		return "TagOptionSome"
	case TagStruct:
		return "TagStruct"
	case TagEnum:
		return "TagEnum"
	case TagArray:
		return "TagArray"
	case TagU128:
		return "TagU128"
	case TagI128:
		return "TagI128"
	case TagU256:
		return "TagU256"
	case TagI256:
		return "TagI256"
	default:
		return fmt.Sprintf("UnknownTag(0x%x)", tag)
	}
}

// Errorf adds the standard "bsatn:" prefix to formatted errors so helpers and
// callers remain consistent with the built-in Err* values.
func Errorf(format string, args ...interface{}) error {
	return fmt.Errorf("bsatn: "+format, args...)
}
