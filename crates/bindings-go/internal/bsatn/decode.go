package bsatn

import (
	"bytes" // Required for bytes.NewReader
	"errors"
	"io"
	"log"
)

// Unmarshal decodes BSATN-encoded data from a buffer into Go primitives
// or structures (Variant, map[string]interface{}, []interface{}),
// returning the decoded value, the number of bytes read, and any error.
func Unmarshal(buf []byte) (interface{}, int, error) {
	// log.Printf("[Unmarshal] entry with direct buffer, len(buf)=%d", len(buf)) // Original log
	bytesReader := bytes.NewReader(buf)
	r := NewReader(bytesReader) // NewReader is from reader.go

	val := unmarshalRecursive(r) // Call the new recursive helper

	if err := r.Error(); err != nil {
		// If r.Error() is EOF, it might mean the buffer was fully consumed as expected.
		// However, if it's an unexpected EOF or other error, that's a problem.
		// The BytesRead() will still be accurate for what was processed before the error.
		if err == io.EOF && r.bytesRead == len(buf) { // Consumed whole buffer, EOF is fine
			return val, r.BytesRead(), nil
		}
		return nil, r.BytesRead(), err
	}
	return val, r.BytesRead(), nil
}

// unmarshalRecursive performs the actual decoding using a Reader.
// It's called by Unmarshal and recursively for nested structures.
func unmarshalRecursive(r *Reader) interface{} {
	if r.Error() != nil { // Check for pre-existing errors on the reader
		return nil
	}

	tag, err := r.ReadTag()
	if err != nil {
		// r.ReadTag() already calls r.recordError()
		log.Printf("[unmarshalRecursive] error reading tag: %v. Reader state: bytesRead=%d, err=%v", err, r.BytesRead(), r.Error())
		return nil
	}
	log.Printf("[unmarshalRecursive] processing tag 0x%x. Reader state: bytesRead=%d", tag, r.BytesRead()-1) // -1 because ReadTag consumed 1 byte

	switch tag {
	case TagBoolFalse:
		return false
	case TagBoolTrue:
		return true
	case TagU8:
		v, err := r.ReadUint8()
		if err != nil {
			return nil
		}
		return v
	case TagI8:
		v, err := r.ReadInt8()
		if err != nil {
			return nil
		}
		return v
	case TagU16:
		v, err := r.ReadUint16()
		if err != nil {
			return nil
		}
		return v
	case TagI16:
		v, err := r.ReadInt16()
		if err != nil {
			return nil
		}
		return v
	case TagU32:
		v, err := r.ReadUint32()
		if err != nil {
			return nil
		}
		return v
	case TagI32:
		v, err := r.ReadInt32()
		if err != nil {
			return nil
		}
		return v
	case TagU64:
		v, err := r.ReadUint64()
		if err != nil {
			return nil
		}
		return v
	case TagI64:
		v, err := r.ReadInt64()
		if err != nil {
			return nil
		}
		return v
	case TagF32:
		v, err := r.ReadFloat32()
		if err != nil {
			return nil
		}
		return v
	case TagF64:
		v, err := r.ReadFloat64()
		if err != nil {
			return nil
		}
		return v
	case TagString:
		v, err := r.ReadString() // Assumes TagString was already consumed by ReadTag
		if err != nil {
			return nil
		}
		return v
	case TagBytes:
		v, err := r.ReadBytesRaw() // Assumes TagBytes was already consumed
		if err != nil {
			return nil
		}
		return v
	case TagList:
		count, err := r.ReadListHeader() // Assumes TagList was consumed by ReadTag
		if err != nil {
			return nil
		}
		log.Printf("[unmarshalRecursive TagList] count=%d. Reader state: bytesRead=%d", count, r.BytesRead())
		items := make([]interface{}, 0, count)
		for i := uint32(0); i < count; i++ {
			log.Printf("[unmarshalRecursive TagList] item %d. Reader state: bytesRead=%d", i, r.BytesRead())
			elem := unmarshalRecursive(r) // Recursive call
			if r.Error() != nil {
				log.Printf("[unmarshalRecursive TagList] error item %d: %v. Reader state: bytesRead=%d", i, r.Error(), r.BytesRead())
				return nil
			}
			items = append(items, elem)
		}
		log.Printf("[unmarshalRecursive TagList] finished. Reader state: bytesRead=%d", r.BytesRead())
		return items
	case TagArray:
		count, err := r.ReadArrayHeader() // Assumes TagArray was consumed
		if err != nil {
			return nil
		}
		log.Printf("[unmarshalRecursive TagArray] count=%d. Reader state: bytesRead=%d", count, r.BytesRead())
		items := make([]interface{}, 0, count)
		for i := uint32(0); i < count; i++ {
			log.Printf("[unmarshalRecursive TagArray] item %d. Reader state: bytesRead=%d", i, r.BytesRead())
			elem := unmarshalRecursive(r) // Recursive call
			if r.Error() != nil {
				log.Printf("[unmarshalRecursive TagArray] error item %d: %v. Reader state: bytesRead=%d", i, r.Error(), r.BytesRead())
				return nil
			}
			items = append(items, elem)
		}
		log.Printf("[unmarshalRecursive TagArray] finished. Reader state: bytesRead=%d", r.BytesRead())
		return items
	case TagOptionNone:
		return nil
	case TagOptionSome:
		// TagOptionSome already consumed by ReadTag
		log.Printf("[unmarshalRecursive TagOptionSome] processing payload. Reader state: bytesRead=%d", r.BytesRead())
		val := unmarshalRecursive(r) // Recursive call for payload
		if r.Error() != nil {
			log.Printf("[unmarshalRecursive TagOptionSome] error processing payload: %v. Reader state: bytesRead=%d", r.Error(), r.BytesRead())
			return nil
		}
		return &val // Return pointer to the interface value, as before
	case TagStruct:
		fieldCount, err := r.ReadStructHeader() // Assumes TagStruct was consumed
		if err != nil {
			return nil
		}
		m := make(map[string]interface{}, fieldCount)
		log.Printf("[unmarshalRecursive TagStruct] fieldCount=%d. Reader state: bytesRead=%d", fieldCount, r.BytesRead())
		for i := uint32(0); i < fieldCount; i++ {
			log.Printf("[unmarshalRecursive TagStruct] field %d. Reader state: bytesRead=%d", i, r.BytesRead())
			name, err := r.ReadFieldName()
			if err != nil {
				log.Printf("[unmarshalRecursive TagStruct] error field %d reading name: %v. Reader state: bytesRead=%d", i, err, r.BytesRead())
				return nil
			}
			log.Printf("[unmarshalRecursive TagStruct] field %d, name=\"%s\". Reader state: bytesRead=%d", i, name, r.BytesRead())
			val := unmarshalRecursive(r) // Recursive call for field value
			if r.Error() != nil {
				log.Printf("[unmarshalRecursive TagStruct] error field %d ('%s') reading value: %v. Reader state: bytesRead=%d", i, name, r.Error(), r.BytesRead())
				return nil
			}
			m[name] = val
		}
		log.Printf("[unmarshalRecursive TagStruct] finished. Reader state: bytesRead=%d", r.BytesRead())
		return m
	case TagEnum:
		idx, err := r.ReadEnumHeader() // Consumes Tag + 4 bytes for index via internal ReadTag then ReadUint32
		if err != nil {
			log.Printf("[unmarshalRecursive TagEnum] error reading enum header: %v. Reader state: bytesRead=%d", err, r.BytesRead())
			return nil // Error already recorded by Reader methods
		}
		log.Printf("[unmarshalRecursive TagEnum] index=%d. Reader state after header: bytesRead=%d", idx, r.BytesRead())

		// Check if this index corresponds to a known unit AlgebraicType kind
		isUnitAlgType := false
		switch atKind(idx) {
		case atString, atBool, atI8, atU8, atI16, atU16, atI32, atU32, atI64, atU64, atI128, atU128, atI256, atU256, atF32, atF64:
			isUnitAlgType = true
		}

		if isUnitAlgType {
			log.Printf("[unmarshalRecursive TagEnum] known unit algebraic type kind for index %d. No payload expected.", idx)
			return Variant{Index: idx} // Payload is implicitly nil
		}

		// For general enums or non-unit AlgebraicTypes that might have a payload.
		// Try to unmarshal a payload. If it results in an immediate EOF with no bytes consumed
		// for the payload itself, then it was a unit variant of a general enum.
		log.Printf("[unmarshalRecursive TagEnum] index %d (not a known unit AT kind), attempting to read payload. Reader state: bytesRead=%d", idx, r.BytesRead())

		bytesReadBeforePayloadAttempt := r.BytesRead()
		payloadVal := unmarshalRecursive(r) // This will attempt to read the next tag and value
		payloadErr := r.Error()
		bytesReadDuringPayloadAttempt := r.BytesRead() - bytesReadBeforePayloadAttempt

		if payloadErr != nil {
			// If EOF occurred AND no actual payload bytes were consumed (i.e., ReadTag for payload got EOF)
			if (errors.Is(payloadErr, io.EOF) || errors.Is(payloadErr, io.ErrUnexpectedEOF)) && bytesReadDuringPayloadAttempt == 0 {
				log.Printf("[unmarshalRecursive TagEnum] index %d, treated as unit variant (EOF with 0 payload bytes read). Clearing reader error.", idx)
				r.err = nil // Clear the EOF error as it indicates a unit variant for a general enum
				return Variant{Index: idx}
			}
			// Otherwise, it's a real error during payload unmarshaling, or EOF after partial read.
			// The error is already recorded in the reader by the recursive call or its sub-operations.
			log.Printf("[unmarshalRecursive TagEnum] error unmarshaling payload for index %d: %v. Bytes read for payload attempt: %d", idx, payloadErr, bytesReadDuringPayloadAttempt)
			return nil
		}

		log.Printf("[unmarshalRecursive TagEnum] successfully unmarshaled payload for index %d. Payload bytes: %d. Reader state: bytesRead=%d", idx, bytesReadDuringPayloadAttempt, r.BytesRead())
		return Variant{Index: idx, Value: payloadVal}

	case TagU128:
		v, err := r.ReadU128Bytes()
		if err != nil {
			return nil
		}
		return v
	case TagI128:
		v, err := r.ReadI128Bytes()
		if err != nil {
			return nil
		}
		return v
	case TagU256:
		v, err := r.ReadU256Bytes()
		if err != nil {
			return nil
		}
		return v
	case TagI256:
		v, err := r.ReadI256Bytes()
		if err != nil {
			return nil
		}
		return v
	default:
		r.recordError(ErrInvalidTag)
		log.Printf("[unmarshalRecursive] error: unknown tag 0x%x. Reader state: bytesRead=%d", tag, r.BytesRead()-1)
		return nil
	}
}
