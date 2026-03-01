package bsatn

import "fmt"

// ErrBufferTooShort is returned when there aren't enough bytes to read.
type ErrBufferTooShort struct {
	ForType  string
	Expected int
	Given    int
}

func (e *ErrBufferTooShort) Error() string {
	return fmt.Sprintf("bsatn: buffer too short for %s: expected %d bytes, got %d", e.ForType, e.Expected, e.Given)
}

// ErrInvalidBool is returned when a bool byte is not 0x00 or 0x01.
type ErrInvalidBool struct {
	Value uint8
}

func (e *ErrInvalidBool) Error() string {
	return fmt.Sprintf("bsatn: invalid bool value: 0x%02x", e.Value)
}

// ErrInvalidTag is returned when a sum type tag is not recognized.
type ErrInvalidTag struct {
	Tag     uint8
	SumName string
}

func (e *ErrInvalidTag) Error() string {
	return fmt.Sprintf("bsatn: invalid tag %d for sum type %s", e.Tag, e.SumName)
}
