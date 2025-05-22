package bsatn

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"reflect"
	"strings"
	"testing"
)

// Helper to create a distinct AlgebraicType for testing, avoiding DeepEqual issues with nil pointers in empty structs.
func makeSchema(kind atKind) AlgebraicType {
	switch kind {
	case atString:
		return StringType()
	case atU32:
		return U32Type()
	case atProduct:
		return ProductTypeOf(ProductElement{Name: stringPtr("field"), Type: U32Type()})
	default:
		return AlgebraicType{Kind: kind}
	}
}

func stringPtr(s string) *string { return &s }
func intPtr(i int) *int          { ptr := i; return &ptr }

// TestRegistry (assuming this test is correct from previous versions)
func TestRegistry(t *testing.T) {
	type MyStruct1 struct{ F int }
	type MyStruct2 struct{ F string }

	schema1 := makeSchema(atString)
	schema2 := makeSchema(atU32)
	schema1Different := makeSchema(atProduct)
	noConstraints := []Constraint{}

	// Using placeholder RefIDs for tests. These should be unique where necessary for conflict testing.
	// For many tests, the exact ID doesn't matter beyond satisfying the signature.
	var currentRefID uint32 = 0
	nextRefID := func() uint32 {
		currentRefID++
		return currentRefID
	}

	t.Run("RegisterAndGet", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0 // Reset for this subtest to keep IDs somewhat predictable if needed
		instance1 := MyStruct1{}
		err := RegisterType(instance1, "MyModule.MyStruct1", nextRefID(), schema1, noConstraints...)
		if err != nil {
			t.Fatalf("RegisterType failed: %v", err)
		}
		info, found := GetTypeInfoByGoType(instance1)
		if !found || info == nil {
			t.Fatal("GetTypeInfoByGoType failed to find registered type")
		}
		if info.SATSName != "MyModule.MyStruct1" {
			t.Errorf("SATSName mismatch")
		}
		if !reflect.DeepEqual(info.Schema, schema1) {
			t.Errorf("Schema mismatch")
		}
		if len(info.Constraints) != 0 {
			t.Errorf("Expected 0 constraints, got %d", len(info.Constraints))
		}
		infoByName, foundName := GetTypeInfoBySATSName("MyModule.MyStruct1")
		if !foundName || infoByName == nil {
			t.Fatal("GetTypeInfoBySATSName failed")
		}
		if !reflect.DeepEqual(info, infoByName) {
			t.Errorf("Lookup mismatch")
		}
	})
	t.Run("RegisterPointerAndGetByValue", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		err := RegisterType(&MyStruct1{}, "MyModule.MyStruct1Ptr", nextRefID(), schema1, noConstraints...)
		if err != nil {
			t.Fatalf("RegisterType with pointer failed: %v", err)
		}
		info, found := GetTypeInfoByGoType(MyStruct1{})
		if !found || info == nil {
			t.Fatal("GetTypeInfoByGoType with value failed")
		}
		if info.SATSName != "MyModule.MyStruct1Ptr" {
			t.Errorf("SATSName mismatch")
		}
	})
	t.Run("ConflictGoType", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		MustRegisterType(MyStruct1{}, "MyModule.MyStruct1", nextRefID(), schema1, noConstraints...)
		refIDForNext := nextRefID() // Use a new RefID for the potentially conflicting registration
		err := RegisterType(MyStruct1{}, "MyModule.MyStruct1DifferentSATS", refIDForNext, schema2, noConstraints...)
		if err == nil {
			t.Error("Expected error for Go type conflict (even with different SATS name/RefID if GoType is same), got nil")
		}
		err = RegisterType(MyStruct1{}, "MyModule.MyStruct1", 1, schema1, noConstraints...) // Assuming RefID 1 was used first
		if err != nil {
			t.Errorf("Re-registering identical type info failed: %v", err)
		}
	})
	t.Run("ConflictSATSName", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		MustRegisterType(MyStruct1{}, "MyModule.SameSATSName", nextRefID(), schema1, noConstraints...)
		err := RegisterType(MyStruct2{}, "MyModule.SameSATSName", nextRefID(), schema2, noConstraints...)
		if err == nil {
			t.Error("Expected error for SATS name conflict, got nil")
		}
		err = RegisterType(MyStruct1{}, "MyModule.SameSATSName", 1, schema1, noConstraints...) // Assuming RefID 1
		if err != nil {
			t.Errorf("Re-registering identical SATS name info failed: %v", err)
		}
	})
	t.Run("ConflictSchemaOnGoType", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		MustRegisterType(MyStruct1{}, "MyModule.MyStruct1", nextRefID(), schema1, noConstraints...)
		err := RegisterType(MyStruct1{}, "MyModule.MyStruct1", 1, schema1Different, noConstraints...)
		if err == nil {
			t.Error("Expected error for schema conflict on same Go type, got nil")
		}
	})
	t.Run("ConflictSchemaOnSATSName", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		refID1 := nextRefID()
		MustRegisterType(MyStruct1{}, "MyModule.MyStruct1", refID1, schema1, noConstraints...)
		refID2 := nextRefID()
		MustRegisterType(MyStruct2{}, "MyModule.MyStruct2WithSchemaConflict", refID2, schema1, noConstraints...)
		err := RegisterType(MyStruct2{}, "MyModule.MyStruct2WithSchemaConflict", refID2, schema1Different, noConstraints...)
		if err == nil {
			t.Error("Expected error for schema conflict on same SATS name, got nil")
		}
	})
	t.Run("RegisterWithConstraints", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		constraints := []Constraint{{Path: "F", Kind: ConstraintKindMinMaxLen, MinLen: 1, MaxLen: 10}}
		MustRegisterType(MyStruct2{}, "MyModule.MyStruct2Constrained", nextRefID(), schema1, constraints...)
		info, found := GetTypeInfoByGoType(MyStruct2{})
		if !found || info == nil {
			t.Fatal("Failed to get info for constrained type")
		}
		if len(info.Constraints) != 1 {
			t.Errorf("Expected 1 constraint, got %d", len(info.Constraints))
		}
		if !reflect.DeepEqual(info.Constraints, constraints) {
			t.Errorf("Constraints mismatch")
		}
	})
	t.Run("ConflictConstraintsOnGoType", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		constraints1 := []Constraint{{Kind: ConstraintKindMinMaxLen, MinLen: 1}}
		constraints2 := []Constraint{{Kind: ConstraintKindMinMaxLen, MinLen: 2}}
		refID := nextRefID()
		MustRegisterType(MyStruct1{}, "MyModule.MyStruct1", refID, schema1, constraints1...)
		err := RegisterType(MyStruct1{}, "MyModule.MyStruct1", refID, schema1, constraints2...)
		if err == nil {
			t.Error("Expected error for constraint conflict, got nil")
		}
		err = RegisterType(MyStruct1{}, "MyModule.MyStruct1", refID, schema1, constraints1...)
		if err != nil {
			t.Errorf("Re-registering identical constraints failed: %v", err)
		}
	})
	t.Run("RegisterNilInstance", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		err := RegisterType(nil, "NilType", nextRefID(), schema1, noConstraints...)
		if err == nil {
			t.Error("Expected error for nil instance, got nil")
		}
	})
	t.Run("GetNonExistent", func(t *testing.T) {
		ClearRegistry()
		_, found := GetTypeInfoByGoType(MyStruct1{})
		if found {
			t.Error("GetTypeInfoByGoType found non-existent type")
		}
		_, found = GetTypeInfoBySATSName("NonExistent.Type")
		if found {
			t.Error("GetTypeInfoBySATSName found non-existent type")
		}
	})
	t.Run("RegisterWithInvalidSchemaKind", func(t *testing.T) {
		ClearRegistry()
		currentRefID = 0
		invalidSchema := AlgebraicType{Kind: atKind(999)} // This will cause a panic if String() is called on it and it's truly invalid for String() range
		err := RegisterType(MyStruct1{}, "MyModule.InvalidSchemaKind", nextRefID(), invalidSchema, noConstraints...)
		if err == nil {
			t.Fatalf("Expected error for invalid schema kind, got nil")
		}
		t.Logf("Got expected error for invalid schema kind: %v", err)
	})
}

// CustomPoint and TestCustomTypeSerialization (assuming correct)
type CustomPoint struct {
	X       int32
	Y       int32
	written bool
	read    bool
}

func (cp *CustomPoint) WriteBSATN(w *Writer) error {
	cp.written = true
	w.WriteInt32(cp.X)
	w.WriteInt32(cp.Y)
	return w.Error()
}
func (cp *CustomPoint) ReadBSATN(r *Reader) error {
	cp.read = true
	var err error
	var tag byte
	tag, err = r.ReadTag()
	if err != nil {
		r.recordError(err)
		return err
	}
	if tag != TagI32 {
		err = fmt.Errorf("exp TagI32 for X, got %x", tag)
		r.recordError(err)
		return err
	}
	cp.X, err = r.ReadInt32()
	if err != nil {
		r.recordError(err)
		return err
	}
	tag, err = r.ReadTag()
	if err != nil {
		r.recordError(err)
		return err
	}
	if tag != TagI32 {
		err = fmt.Errorf("exp TagI32 for Y, got %x", tag)
		r.recordError(err)
		return err
	}
	cp.Y, err = r.ReadInt32()
	if err != nil {
		r.recordError(err)
		return err
	}
	return r.Error()
}
func TestCustomTypeSerialization(t *testing.T) {
	ClearRegistry()
	t.Run("MarshalCustomType", func(t *testing.T) {
		cp := &CustomPoint{X: 10, Y: 20}
		bsatnBytes, err := Marshal(cp)
		if err != nil {
			t.Fatalf("Marshal failed for CustomPoint: %v", err)
		}
		if !cp.written {
			t.Error("CustomPoint.WriteBSATN was not called")
		}
		expectedBytes := []byte{TagI32, 0x0a, 0, 0, 0, TagI32, 0x14, 0, 0, 0}
		if !bytes.Equal(bsatnBytes, expectedBytes) {
			t.Errorf("Marshaled bytes mismatch Got: %x Want: %x", bsatnBytes, expectedBytes)
		}
	})
	t.Run("UnmarshalIntoCustomType", func(t *testing.T) {
		bsatnBytes := []byte{TagI32, 0x0a, 0, 0, 0, TagI32, 0x14, 0, 0, 0}
		cp := &CustomPoint{}
		err := UnmarshalInto(bsatnBytes, cp)
		if err != nil {
			t.Fatalf("UnmarshalInto failed: %v", err)
		}
		if !cp.read {
			t.Error("CustomPoint.ReadBSATN was not called")
		}
		if cp.X != 10 || cp.Y != 20 {
			t.Errorf("Data mismatch: got X=%d,Y=%d", cp.X, cp.Y)
		}
	})
	t.Run("UnmarshalIntoCustomType_NotEnoughData", func(t *testing.T) {
		bsatnBytes := []byte{TagI32, 0x0a, 0, 0, 0, TagI32, 0x14, 0, 0}
		cp := &CustomPoint{}
		err := UnmarshalInto(bsatnBytes, cp)
		if err == nil {
			t.Fatal("Expected error for not enough data")
		}
		if !errors.Is(err, io.ErrUnexpectedEOF) && !errors.Is(err, io.EOF) && !errors.Is(err, ErrBufferTooSmall) {
			t.Errorf("Expected io.ErrUnexpectedEOF, io.EOF or ErrBufferTooSmall, got: %v", err)
		}
		if !cp.read {
			t.Error("CustomPoint.ReadBSATN was not called with partial data")
		}
	})
}

func TestConstraintValidation(t *testing.T) {
	ClearRegistry()

	type NestedConstrained struct {
		Value string `bsatn:"value"`
	}
	type ConstrainedStruct struct {
		Name        string            `bsatn:"name"`
		Tags        []string          `bsatn:"tags"`
		Description *string           `bsatn:"description"`
		Count       int               `bsatn:"count"`
		Amount      uint              `bsatn:"amount"`
		Price       float32           `bsatn:"price"`
		Nested      NestedConstrained `bsatn:"nested"`
		MaybeCount  *int              `bsatn:"maybe_count"`
	}

	schema := ProductTypeOf(
		ProductElement{Name: stringPtr("Name"), Type: StringType()},
		ProductElement{Name: stringPtr("Tags"), Type: ArrayTypeOf(StringType())},
		ProductElement{Name: stringPtr("Description"), Type: OptionTypeOf(StringType())},
		ProductElement{Name: stringPtr("Count"), Type: I32Type()},
		ProductElement{Name: stringPtr("Amount"), Type: U32Type()},
		ProductElement{Name: stringPtr("Price"), Type: F32Type()},
		ProductElement{Name: stringPtr("Nested"), Type: ProductTypeOf(ProductElement{Name: stringPtr("Value"), Type: StringType()})},
		ProductElement{Name: stringPtr("MaybeCount"), Type: OptionTypeOf(I32Type())},
	)

	baseSatsName := "My.ConstrainedStruct"
	baseConstraints := []Constraint{
		{Path: "Name", Kind: ConstraintKindMinMaxLen, MinLen: 3, MaxLen: 10},
		{Path: "Name", Kind: ConstraintKindPattern, RegexPattern: "^[a-zA-Z]+$"},
		{Path: "Tags", Kind: ConstraintKindMinMaxLen, MinLen: 1, MaxLen: 3},
		{Path: "Description", Kind: ConstraintKindMinMaxLen, MinLen: 0, MaxLen: 5},
		{Path: "Nested.Value", Kind: ConstraintKindMinMaxLen, MinLen: 1, MaxLen: 5},
		{Path: "Count", Kind: ConstraintKindMinMax, Min: int64(1), Max: int64(100)},
		{Path: "Amount", Kind: ConstraintKindMinMax, Min: uint64(0), Max: uint64(1000)},
		{Path: "Price", Kind: ConstraintKindMinMax, Min: float32(0.99), Max: float32(99.99)},
		{Path: "MaybeCount", Kind: ConstraintKindMinMax, Min: int64(1), Max: int64(5)},
	}

	MustRegisterType(ConstrainedStruct{}, baseSatsName, 1, schema, baseConstraints...)
	baseInfo, foundBase := GetTypeInfoBySATSName(baseSatsName)
	if !foundBase {
		t.Fatalf("Base registration for %s failed", baseSatsName)
	}

	// Pre-define custom info structs to avoid complex literals in test case definitions
	customInfoForPatternOnInt := func() *RegisteredTypeInfo {
		cons := make([]Constraint, len(baseConstraints))
		copy(cons, baseConstraints)
		cons = append(cons, Constraint{Path: "Count", Kind: ConstraintKindPattern, RegexPattern: "^[0-9]+$"})
		return &RegisteredTypeInfo{
			GoType:      reflect.TypeOf(ConstrainedStruct{}),
			SATSName:    "My.ConstrainedStruct.TempPatternOnInt",
			Schema:      schema,
			Constraints: cons,
		}
	}()

	invalidPathConstraintDetails := []Constraint{{Path: "NonExistentField.Value", Kind: ConstraintKindMinMaxLen, MinLen: 1}}
	customInfoForInvalidPath := &RegisteredTypeInfo{
		GoType:      reflect.TypeOf(ConstrainedStruct{}),
		SATSName:    "My.ConstrainedStruct.TempInvalidPath",
		Schema:      schema,
		Constraints: invalidPathConstraintDetails,
	}

	type constraintTestCase struct {
		name             string
		instance         ConstrainedStruct
		customInfo       *RegisteredTypeInfo
		expectErrorCount int
		checkError       func(t *testing.T, errs []ValidationError)
	}

	testCases := []constraintTestCase{
		{
			name:             "ValidInstance",
			instance:         ConstrainedStruct{Name: "ValidName", Tags: []string{"tag1"}, Description: stringPtr("valid"), Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}, MaybeCount: intPtr(3)},
			expectErrorCount: 0,
		},
		{
			name:             "NameTooShort",
			instance:         ConstrainedStruct{Name: "No", Tags: []string{"tag1"}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				if len(errs) > 0 && (errs[0].Path != "Name" || errs[0].ConstraintKind != ConstraintKindMinMaxLen) {
					t.Errorf("Expected Name MinMaxLen error, got Path: %s, Kind: %s", errs[0].Path, errs[0].ConstraintKind)
				}
			},
		},
		{
			name:             "NameTooLong",
			instance:         ConstrainedStruct{Name: "ThisNameIsWayTooLong", Tags: []string{"tag1"}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 1,
		},
		{
			name:             "NameInvalidPattern",
			instance:         ConstrainedStruct{Name: "Name123", Tags: []string{"tag1"}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				if len(errs) > 0 && (errs[0].Path != "Name" || errs[0].ConstraintKind != ConstraintKindPattern) {
					t.Errorf("Expected Name Pattern error, got Path: %s, Kind: %s", errs[0].Path, errs[0].ConstraintKind)
				}
			},
		},
		{
			name:             "TagsTooFew",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 1,
		},
		{
			name:             "TagsTooMany",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"a", "b", "c", "d"}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 1,
		},
		{
			name:             "NilDescriptionIsValidByZeroMinLen",
			instance:         ConstrainedStruct{Name: "ValidName", Tags: []string{"tag1"}, Description: nil, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}, MaybeCount: intPtr(3)},
			expectErrorCount: 0,
		},
		{
			name:             "DescriptionTooLong",
			instance:         ConstrainedStruct{Name: "ValidName", Tags: []string{"tag1"}, Description: stringPtr("toolong"), Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 1,
		},
		{
			name:             "NestedValueValid",
			instance:         ConstrainedStruct{Name: "ValidName", Tags: []string{"tag1"}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nest"}},
			expectErrorCount: 0,
		},
		{
			name:             "NestedValueTooLong",
			instance:         ConstrainedStruct{Name: "ValidName", Tags: []string{"tag1"}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "nesttoolong"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				if len(errs) > 0 && (errs[0].Path != "Nested.Value" || errs[0].ConstraintKind != ConstraintKindMinMaxLen) {
					t.Errorf("Expected Nested.Value MinMaxLen error, got Path: %s, Kind: %s, Msg: %s", errs[0].Path, errs[0].ConstraintKind, errs[0].Message)
				}
			},
		},
		{
			name:             "MultipleErrors",
			instance:         ConstrainedStruct{Name: "N@", Tags: []string{}, Count: 50, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "toolong"}},
			expectErrorCount: 4,
		},
		{
			name:             "CountBelowMin",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 0, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				checkMinMaxError(t, errs, "Count", "value 0 is less than minimum 1")
			},
		},
		{
			name:             "CountAboveMax",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 101, Amount: 100, Price: 19.99, Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				checkMinMaxError(t, errs, "Count", "value 101 is greater than maximum 100")
			},
		},
		{
			name:             "AmountAboveMax",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 50, Amount: 1001, Price: 19.99, Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				checkMinMaxError(t, errs, "Amount", "value 1001 is greater than maximum 1000")
			},
		},
		{
			name:             "PriceBelowMin",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 50, Amount: 100, Price: 0.98, Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				checkMinMaxError(t, errs, "Price", "value 0.98 is less than minimum 0.99")
			},
		},
		{
			name:             "PriceAboveMax",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 50, Amount: 100, Price: 100.00, Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				checkMinMaxError(t, errs, "Price", "value 100 is greater than maximum 99.99")
			},
		},
		{
			name:             "MaybeCountNilFailsMinConstraint_WithBaseConstraints",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 50, Amount: 100, Price: 19.99, MaybeCount: nil, Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 0,
		},
		{
			name:             "MaybeCountValid",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 50, Amount: 100, Price: 19.99, MaybeCount: intPtr(3), Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 0,
		},
		{
			name:             "MaybeCountTooLow",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 50, Amount: 100, Price: 19.99, MaybeCount: intPtr(0), Nested: NestedConstrained{Value: "ok"}},
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				checkMinMaxError(t, errs, "MaybeCount", "value 0 is less than minimum 1")
			},
		},
		{
			name:             "ConstraintOnUnsupportedType",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"t1"}, Count: 5, Nested: NestedConstrained{Value: "nest"}},
			customInfo:       customInfoForPatternOnInt,
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				if len(errs) > 0 && !strings.Contains(errs[0].Message, "Pattern constraint applied to non-numeric type int") {
					t.Errorf("Expected unsupported type error for pattern on int, got: %s", errs[0].Message)
				}
			},
		},
		{
			name:             "InvalidPath",
			instance:         ConstrainedStruct{Name: "Valid", Tags: []string{"tag1"}, Nested: NestedConstrained{Value: "nest"}},
			customInfo:       customInfoForInvalidPath,
			expectErrorCount: 1,
			checkError: func(t *testing.T, errs []ValidationError) {
				if len(errs) > 0 && !strings.Contains(errs[0].Message, "Error accessing field for constraint") {
					t.Errorf("Expected path error, got: %s", errs[0].Message)
				}
			},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			infoToUse := baseInfo
			if tc.customInfo != nil {
				infoToUse = tc.customInfo
			}
			if infoToUse == nil {
				t.Fatalf("RegisteredTypeInfo not available for test case: %s. baseInfo was nil or customInfo was nil but expected.", tc.name)
			}

			errs := Validate(&tc.instance, infoToUse)
			if len(errs) != tc.expectErrorCount {
				t.Errorf("Expected %d validation errors, got %d:", tc.expectErrorCount, len(errs))
				for _, ve := range errs {
					t.Errorf("  - %v (%s: %s)", ve, ve.Path, ve.ConstraintKind)
				}
			}
			if tc.checkError != nil {
				tc.checkError(t, errs)
			}
		})
	}
}

// Helper for checking MinMax errors
func checkMinMaxError(t *testing.T, errs []ValidationError, expectedPath, expectedMsgPart string) {
	t.Helper()
	if len(errs) == 0 {
		t.Errorf("Expected a MinMax validation error for %s, got none", expectedPath)
		return
	}
	found := false
	for _, err := range errs {
		if err.Path == expectedPath && err.ConstraintKind == ConstraintKindMinMax && strings.Contains(err.Message, expectedMsgPart) {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("Expected error for %s containing '%s', got errors: %v", expectedPath, expectedMsgPart, errs)
	}
}
