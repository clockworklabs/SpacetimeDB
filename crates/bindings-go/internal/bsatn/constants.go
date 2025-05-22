package bsatn

const (
	TagBoolFalse  byte = 0x01
	TagBoolTrue   byte = 0x02
	TagU8         byte = 0x03
	TagI8         byte = 0x04
	TagU16        byte = 0x05
	TagI16        byte = 0x06
	TagU32        byte = 0x07
	TagI32        byte = 0x08
	TagU64        byte = 0x09
	TagI64        byte = 0x0A
	TagF32        byte = 0x0B
	TagF64        byte = 0x0C
	TagString     byte = 0x0D // length prefixed u32 LE
	TagBytes      byte = 0x0E // length prefixed u32 LE
	TagList       byte = 0x0F // length-prefixed list of elements
	TagOptionNone byte = 0x10
	TagOptionSome byte = 0x11
	TagStruct     byte = 0x12    // struct: fieldCount u32 then nameLen u8 + name bytes + value
	TagEnum       byte = 0x13    // enum: variantIndex u32 + payload(optional)
	TagArray      byte = 0x14    // homogeneous array/slice
	TagU128       byte = 0x15    // 16 bytes
	TagI128       byte = 0x16    // 16 bytes
	TagU256       byte = 0x17    // 32 bytes
	TagI256       byte = 0x18    // 32 bytes
	MaxPayloadLen int  = 1 << 20 // 1 MiB safety cap for strings/byte slices
)
