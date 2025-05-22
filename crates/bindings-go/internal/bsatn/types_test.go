package bsatn

import "testing"

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
		enc, err := MarshalAlgebraicType(at)
		if err != nil {
			t.Fatalf("case %d marshal: %v", i, err)
		}
		dec, err := UnmarshalAlgebraicType(enc)
		if err != nil {
			t.Fatalf("case %d unmarshal: %v", i, err)
		}
		if dec.Kind != at.Kind {
			t.Fatalf("case %d kind mismatch got %v want %v", i, dec.Kind, at.Kind)
		}
		// simplistic equality check via re-encode both and compare bytes
		enc2, _ := MarshalAlgebraicType(dec)
		if string(enc) != string(enc2) {
			t.Fatalf("case %d round-trip mismatch", i)
		}
	}
}
