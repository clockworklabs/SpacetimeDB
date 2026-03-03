package main

// ReturnStruct is returned by procedures.
type ReturnStruct struct {
	A uint32
	B string
}

// ReturnEnum is a sum type returned by procedures.
//stdb:sumtype
type ReturnEnum interface {
	returnEnumTag() uint8
}

//stdb:variant of=ReturnEnum name=A
type ReturnEnumA struct {
	Value uint32
}

func (ReturnEnumA) returnEnumTag() uint8 { return 0 }

//stdb:variant of=ReturnEnum name=B
type ReturnEnumB struct {
	Value string
}

func (ReturnEnumB) returnEnumTag() uint8 { return 1 }
