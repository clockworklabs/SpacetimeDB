package bsatn

import "errors"

var (
	ErrInvalidTag     = errors.New("bsatn: invalid type tag")
	ErrBufferTooSmall = errors.New("bsatn: buffer too small")
	ErrInvalidUTF8    = errors.New("bsatn: invalid utf8 string")
	ErrOverflow       = errors.New("bsatn: integer overflow")
	ErrInvalidFloat   = errors.New("bsatn: invalid float value (NaN or Inf)")
	ErrTooLarge       = errors.New("bsatn: payload too large")
)
