package bsatn

import (
	"bytes"
	"testing"
)

func strptr(s string) *string { return &s }

func TestAlgebraicTypeRoundTrip(t *testing.T) {
	cases := []AlgebraicType{
		StringType(),
		BoolType(),
		RefType(42),
		// TODO: array encoding of nested AlgebraicType requires custom marshal; skip for now
		//ArrayTypeOf(U8Type()),
		// Option<String>
		SumTypeOf(
			SumVariant{Name: strptr("some"), Type: StringType()},
			SumVariant{Name: strptr("none"), Type: ProductTypeOf()}, // unit product
		),
		// product { id: U32 }
		ProductTypeOf(
			ProductElement{Name: strptr("id"), Type: U32Type()},
		),
	}

	for i, at := range cases {
		var initialBuf bytes.Buffer
		wInitial := NewWriter(&initialBuf)
		MarshalAlgebraicType(wInitial, at)
		if err := wInitial.Error(); err != nil {
			t.Fatalf("case %d MarshalAlgebraicType failed: %v", i, err)
		}
		enc := initialBuf.Bytes()

		dec, err := UnmarshalAlgebraicType(enc)
		if err != nil {
			t.Fatalf("case %d unmarshal: %v\nEncoded: %x", i, err, enc)
		}
		if dec.Kind != at.Kind {
			t.Fatalf("case %d kind mismatch got %v (%s) want %v (%s)\nEncoded: %x\nDecoded: %#v", i, dec.Kind, dec.Kind.String(), at.Kind, at.Kind.String(), enc, dec)
		}

		var roundTripBuf bytes.Buffer
		wRoundTrip := NewWriter(&roundTripBuf)
		MarshalAlgebraicType(wRoundTrip, dec)
		if err := wRoundTrip.Error(); err != nil {
			t.Fatalf("case %d MarshalAlgebraicType on decoded value failed: %v", i, err)
		}
		enc2 := roundTripBuf.Bytes()

		if !bytes.Equal(enc, enc2) {
			t.Logf("Case %d Original AT: %#v", i, at)
			t.Logf("Case %d Decoded AT:  %#v", i, dec)
			t.Logf("Case %d Original Encoded: %x", i, enc)
			t.Logf("Case %d Roundtrip Encoded: %x", i, enc2)
			t.Fatalf("case %d round-trip mismatch for kind %s", i, at.Kind.String())
		}
	}
}
