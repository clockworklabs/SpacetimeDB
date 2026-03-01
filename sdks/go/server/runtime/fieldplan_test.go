package runtime_test

import (
	"reflect"
	"testing"
	"unsafe"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// --- Test struct types ---

type primitiveStruct struct {
	B  bool
	U8 uint8
	U16 uint16
	U32 uint32
	U64 uint64
	I8 int8
	I16 int16
	I32 int32
	I64 int64
	F32 float32
	F64 float64
}

type stringStruct struct {
	Name  string
	Value string
}

type nestedInner struct {
	X uint32
	Y uint64
}

type nestedStruct struct {
	Id    uint32
	Inner nestedInner
	Name  string
}

type sliceStruct struct {
	Values []uint32
	Names  []string
}

type byteSliceStruct struct {
	Data []byte
}

type structSliceStruct struct {
	Items []nestedInner
}

type optionStruct struct {
	Opt *uint32
	Str *string
}

type benchmarkLikeStruct struct {
	Id   uint32
	Age  uint64
	Name string
}

type u64OnlyStruct struct {
	Id uint32
	X  uint64
	Y  uint64
}

type identityStruct struct {
	Id       uint32
	Identity types.Identity
}

type timestampStruct struct {
	Id uint32
	Ts types.Timestamp
}

type uint128Struct struct {
	Val types.Uint128
}

// --- Sum type test setup ---

type testSumIface interface {
	isTestSum()
}

type testSumVariantA struct {
	Value uint32
}

func (testSumVariantA) isTestSum() {}

type testSumVariantB struct {
	Name string
}

func (testSumVariantB) isTestSum() {}

type testSumVariantUnit struct{}

func (testSumVariantUnit) isTestSum() {}

type sumTypeStruct struct {
	Id  uint32
	Sum testSumIface
}

// --- Simple enum test setup ---

type testSimpleEnum uint8

const (
	testEnumA testSimpleEnum = 0
	testEnumB testSimpleEnum = 1
	testEnumC testSimpleEnum = 2
)

type simpleEnumStruct struct {
	Id   uint32
	Kind testSimpleEnum
}

// --- Registration ---

func init() {
	runtime.RegisterSumType(
		reflect.TypeOf((*testSumIface)(nil)).Elem(),
		[]runtime.SumTypeVariantDef{
			{Name: "A", Type: reflect.TypeOf(testSumVariantA{})},
			{Name: "B", Type: reflect.TypeOf(testSumVariantB{})},
			{Name: "Unit", Type: reflect.TypeOf(testSumVariantUnit{})},
		},
	)
	runtime.RegisterSimpleEnum(
		reflect.TypeOf(testSimpleEnum(0)),
		"A", "B", "C",
	)
}

// --- Helper: encode with reflect, encode with plan, compare ---

func reflectEncodeHelper(t *testing.T, v any) []byte {
	t.Helper()
	w := bsatn.NewWriter(128)
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Ptr {
		rv = rv.Elem()
	}
	runtime.ExportedReflectEncodeValue(w, rv)
	return w.Bytes()
}

func planEncodeHelper(t *testing.T, plan *runtime.ExportedStructPlan, v any) []byte {
	t.Helper()
	w := bsatn.NewWriter(128)
	// Create an addressable copy via reflect.New.
	rv := reflect.ValueOf(v)
	if rv.Kind() == reflect.Ptr {
		rv = rv.Elem()
	}
	cp := reflect.New(rv.Type())
	cp.Elem().Set(rv)
	plan.PlanEncode(w, unsafe.Pointer(cp.Pointer()))
	return w.Bytes()
}

func planDecodeHelper(t *testing.T, plan *runtime.ExportedStructPlan, data []byte, target any) {
	t.Helper()
	r := bsatn.NewReader(data)
	rv := reflect.ValueOf(target)
	require.Equal(t, reflect.Ptr, rv.Kind(), "target must be a pointer")
	ptr := rv.Pointer()
	err := plan.PlanDecode(r, unsafe.Pointer(ptr))
	require.NoError(t, err)
}

// --- Tests ---

func TestFieldPlanPrimitives(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(primitiveStruct{}))
	require.NotNil(t, plan)

	input := primitiveStruct{
		B: true, U8: 42, U16: 1000, U32: 100000, U64: 9999999999,
		I8: -1, I16: -100, I32: -100000, I64: -9999999999,
		F32: 3.14, F64: 2.71828,
	}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes, "plan encode should match reflect encode")

	var decoded primitiveStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded, "round-trip decode should match input")
}

func TestFieldPlanStrings(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(stringStruct{}))

	input := stringStruct{Name: "hello", Value: "world"}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded stringStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanEmptyString(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(stringStruct{}))

	input := stringStruct{Name: "", Value: ""}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded stringStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanNestedStruct(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(nestedStruct{}))

	input := nestedStruct{Id: 1, Inner: nestedInner{X: 10, Y: 20}, Name: "test"}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded nestedStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanSliceOfPrimitives(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(sliceStruct{}))

	input := sliceStruct{Values: []uint32{1, 2, 3}, Names: []string{"a", "b", "c"}}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded sliceStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanNilSlice(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(sliceStruct{}))

	input := sliceStruct{Values: nil, Names: nil}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded sliceStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	// nil slices decode as empty slices with zero length
	assert.Equal(t, 0, len(decoded.Values))
	assert.Equal(t, 0, len(decoded.Names))
}

func TestFieldPlanByteSlice(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(byteSliceStruct{}))

	input := byteSliceStruct{Data: []byte{0x01, 0x02, 0x03, 0xFF}}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded byteSliceStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input.Data, decoded.Data)
}

func TestFieldPlanStructSlice(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(structSliceStruct{}))

	input := structSliceStruct{Items: []nestedInner{
		{X: 1, Y: 2},
		{X: 3, Y: 4},
	}}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded structSliceStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanOptionSome(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(optionStruct{}))

	v := uint32(42)
	s := "hello"
	input := optionStruct{Opt: &v, Str: &s}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded optionStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	require.NotNil(t, decoded.Opt)
	assert.Equal(t, uint32(42), *decoded.Opt)
	require.NotNil(t, decoded.Str)
	assert.Equal(t, "hello", *decoded.Str)
}

func TestFieldPlanOptionNone(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(optionStruct{}))

	input := optionStruct{Opt: nil, Str: nil}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded optionStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Nil(t, decoded.Opt)
	assert.Nil(t, decoded.Str)
}

func TestFieldPlanBenchmarkStruct(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(benchmarkLikeStruct{}))

	input := benchmarkLikeStruct{Id: 1, Age: 25, Name: "Alice"}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded benchmarkLikeStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanU64OnlyStruct(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(u64OnlyStruct{}))

	input := u64OnlyStruct{Id: 1, X: 100, Y: 200}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded u64OnlyStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input, decoded)
}

func TestFieldPlanIdentity(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(identityStruct{}))

	var idBytes [32]byte
	for i := range idBytes {
		idBytes[i] = byte(i)
	}
	input := identityStruct{Id: 1, Identity: types.NewIdentity(idBytes)}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded identityStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input.Id, decoded.Id)
	assert.Equal(t, input.Identity.Bytes(), decoded.Identity.Bytes())
}

func TestFieldPlanTimestamp(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(timestampStruct{}))

	input := timestampStruct{Id: 1, Ts: types.NewTimestamp(1234567890)}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded timestampStruct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input.Id, decoded.Id)
	assert.Equal(t, input.Ts.Microseconds(), decoded.Ts.Microseconds())
}

func TestFieldPlanUint128(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(uint128Struct{}))

	input := uint128Struct{Val: types.NewUint128(0xDEADBEEF, 0xCAFEBABE)}

	reflectBytes := reflectEncodeHelper(t, input)
	planBytes := planEncodeHelper(t, plan, input)
	assert.Equal(t, reflectBytes, planBytes)

	var decoded uint128Struct
	planDecodeHelper(t, plan, planBytes, &decoded)
	assert.Equal(t, input.Val.Bytes(), decoded.Val.Bytes())
}

func TestFieldPlanSumType(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(sumTypeStruct{}))

	// Test variant A (tag 0)
	inputA := sumTypeStruct{Id: 1, Sum: testSumVariantA{Value: 42}}
	reflectBytesA := reflectEncodeHelper(t, inputA)
	planBytesA := planEncodeHelper(t, plan, inputA)
	assert.Equal(t, reflectBytesA, planBytesA, "sum type variant A encode mismatch")

	var decodedA sumTypeStruct
	planDecodeHelper(t, plan, planBytesA, &decodedA)
	assert.Equal(t, inputA.Id, decodedA.Id)
	varA, ok := decodedA.Sum.(testSumVariantA)
	require.True(t, ok, "expected testSumVariantA")
	assert.Equal(t, uint32(42), varA.Value)

	// Test variant B (tag 1)
	inputB := sumTypeStruct{Id: 2, Sum: testSumVariantB{Name: "hello"}}
	reflectBytesB := reflectEncodeHelper(t, inputB)
	planBytesB := planEncodeHelper(t, plan, inputB)
	assert.Equal(t, reflectBytesB, planBytesB, "sum type variant B encode mismatch")

	var decodedB sumTypeStruct
	planDecodeHelper(t, plan, planBytesB, &decodedB)
	assert.Equal(t, inputB.Id, decodedB.Id)
	varB, ok := decodedB.Sum.(testSumVariantB)
	require.True(t, ok, "expected testSumVariantB")
	assert.Equal(t, "hello", varB.Name)

	// Test unit variant (tag 2)
	inputUnit := sumTypeStruct{Id: 3, Sum: testSumVariantUnit{}}
	reflectBytesUnit := reflectEncodeHelper(t, inputUnit)
	planBytesUnit := planEncodeHelper(t, plan, inputUnit)
	assert.Equal(t, reflectBytesUnit, planBytesUnit, "sum type unit variant encode mismatch")

	var decodedUnit sumTypeStruct
	planDecodeHelper(t, plan, planBytesUnit, &decodedUnit)
	assert.Equal(t, inputUnit.Id, decodedUnit.Id)
	_, ok = decodedUnit.Sum.(testSumVariantUnit)
	require.True(t, ok, "expected testSumVariantUnit")
}

func TestFieldPlanSimpleEnum(t *testing.T) {
	plan := runtime.ExportedBuildStructPlan(reflect.TypeOf(simpleEnumStruct{}))

	for _, tc := range []struct {
		name  string
		input simpleEnumStruct
	}{
		{"A", simpleEnumStruct{Id: 1, Kind: testEnumA}},
		{"B", simpleEnumStruct{Id: 2, Kind: testEnumB}},
		{"C", simpleEnumStruct{Id: 3, Kind: testEnumC}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			reflectBytes := reflectEncodeHelper(t, tc.input)
			planBytes := planEncodeHelper(t, plan, tc.input)
			assert.Equal(t, reflectBytes, planBytes)

			var decoded simpleEnumStruct
			planDecodeHelper(t, plan, planBytes, &decoded)
			assert.Equal(t, tc.input, decoded)
		})
	}
}

func TestFieldPlanCaching(t *testing.T) {
	plan1 := runtime.ExportedBuildStructPlan(reflect.TypeOf(benchmarkLikeStruct{}))
	plan2 := runtime.ExportedBuildStructPlan(reflect.TypeOf(benchmarkLikeStruct{}))
	assert.True(t, plan1 == plan2, "buildStructPlan should return cached plan")
}
