package runtime_test

import (
	"errors"
	"reflect"
	"testing"
	"unsafe"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/runtime"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// ptrOf returns an unsafe.Pointer to v.
func ptrOf[T any](v *T) unsafe.Pointer {
	return unsafe.Pointer(v)
}

// --- Mock ReducerContext ---

type mockReducerContext struct {
	sender types.Identity
}

func (m *mockReducerContext) Sender() types.Identity       { return m.sender }
func (m *mockReducerContext) ConnectionId() types.ConnectionId { return nil }
func (m *mockReducerContext) Timestamp() types.Timestamp   { return types.NewTimestamp(0) }
func (m *mockReducerContext) Db() any                      { return nil }

func newMockCtx() reducer.ReducerContext {
	return &mockReducerContext{}
}

// --- Test: 0-arg reducer (direct type assertion path) ---

func TestDispatchZeroArgReducer(t *testing.T) {
	runtime.ExportedClearReducers()

	var called bool
	runtime.RegisterReducer("test_zero_arg", func(_ reducer.ReducerContext) {
		called = true
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_zero_arg")
	require.NotNil(t, dispatch)

	err := dispatch(newMockCtx(), nil)
	require.NoError(t, err)
	assert.True(t, called)
}

func TestDispatchZeroArgReducerWithError(t *testing.T) {
	runtime.ExportedClearReducers()

	expectedErr := errors.New("test error")
	runtime.RegisterReducer("test_zero_arg_err", func(_ reducer.ReducerContext) error {
		return expectedErr
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_zero_arg_err")
	require.NotNil(t, dispatch)

	err := dispatch(newMockCtx(), nil)
	assert.Equal(t, expectedErr, err)
}

// --- Test: primitive arg reducers ---

func TestDispatchPrimitiveArgs(t *testing.T) {
	runtime.ExportedClearReducers()

	var gotId uint32
	var gotAge uint64
	var gotName string
	runtime.RegisterReducer("test_primitives", func(_ reducer.ReducerContext, id uint32, age uint64, name string) {
		gotId = id
		gotAge = age
		gotName = name
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_primitives")
	require.NotNil(t, dispatch)

	// Encode args as BSATN product: (u32, u64, string)
	w := bsatn.NewWriter(64)
	w.PutU32(42)
	w.PutU64(100)
	w.PutString("hello")

	err := dispatch(newMockCtx(), w.Bytes())
	require.NoError(t, err)
	assert.Equal(t, uint32(42), gotId)
	assert.Equal(t, uint64(100), gotAge)
	assert.Equal(t, "hello", gotName)
}

func TestDispatchPrimitiveArgsCalledTwice(t *testing.T) {
	runtime.ExportedClearReducers()

	var gotId uint32
	var gotName string
	runtime.RegisterReducer("test_reuse", func(_ reducer.ReducerContext, id uint32, name string) {
		gotId = id
		gotName = name
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_reuse")
	require.NotNil(t, dispatch)

	// First call
	w := bsatn.NewWriter(64)
	w.PutU32(1)
	w.PutString("first")
	err := dispatch(newMockCtx(), w.Bytes())
	require.NoError(t, err)
	assert.Equal(t, uint32(1), gotId)
	assert.Equal(t, "first", gotName)

	// Second call — verifies pre-allocated storage reuse works
	w.Reset()
	w.PutU32(2)
	w.PutString("second")
	err = dispatch(newMockCtx(), w.Bytes())
	require.NoError(t, err)
	assert.Equal(t, uint32(2), gotId)
	assert.Equal(t, "second", gotName)
}

// --- Test: struct arg reducer ---

func TestDispatchStructArg(t *testing.T) {
	runtime.ExportedClearReducers()

	type testRow struct {
		Id   uint32
		Age  uint64
		Name string
	}

	var gotRow testRow
	runtime.RegisterReducer("test_struct", func(_ reducer.ReducerContext, row testRow) {
		gotRow = row
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_struct")
	require.NotNil(t, dispatch)

	// Encode struct as BSATN product fields
	w := bsatn.NewWriter(64)
	w.PutU32(7)
	w.PutU64(30)
	w.PutString("Alice")

	err := dispatch(newMockCtx(), w.Bytes())
	require.NoError(t, err)
	assert.Equal(t, uint32(7), gotRow.Id)
	assert.Equal(t, uint64(30), gotRow.Age)
	assert.Equal(t, "Alice", gotRow.Name)
}

// --- Test: slice arg reducer ---

func TestDispatchSliceArg(t *testing.T) {
	runtime.ExportedClearReducers()

	type testItem struct {
		X uint32
		Y uint64
	}

	var gotItems []testItem
	runtime.RegisterReducer("test_slice", func(_ reducer.ReducerContext, items []testItem) {
		gotItems = items
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_slice")
	require.NotNil(t, dispatch)

	// Encode []testItem as BSATN: array_len + N products
	w := bsatn.NewWriter(128)
	w.PutArrayLen(2)
	w.PutU32(10)
	w.PutU64(20)
	w.PutU32(30)
	w.PutU64(40)

	err := dispatch(newMockCtx(), w.Bytes())
	require.NoError(t, err)
	require.Len(t, gotItems, 2)
	assert.Equal(t, uint32(10), gotItems[0].X)
	assert.Equal(t, uint64(20), gotItems[0].Y)
	assert.Equal(t, uint32(30), gotItems[1].X)
	assert.Equal(t, uint64(40), gotItems[1].Y)
}

// --- Test: error-returning reducer with args ---

func TestDispatchWithErrorReturn(t *testing.T) {
	runtime.ExportedClearReducers()

	runtime.RegisterReducer("test_err_args", func(_ reducer.ReducerContext, n uint32) error {
		if n == 0 {
			return errors.New("cannot be zero")
		}
		return nil
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_err_args")
	require.NotNil(t, dispatch)

	// Call with n=0 should return error
	w := bsatn.NewWriter(4)
	w.PutU32(0)
	err := dispatch(newMockCtx(), w.Bytes())
	assert.EqualError(t, err, "cannot be zero")

	// Call with n=1 should succeed
	w.Reset()
	w.PutU32(1)
	err = dispatch(newMockCtx(), w.Bytes())
	assert.NoError(t, err)
}

// --- Test: single string arg ---

func TestDispatchStringArg(t *testing.T) {
	runtime.ExportedClearReducers()

	var gotName string
	runtime.RegisterReducer("test_string", func(_ reducer.ReducerContext, name string) {
		gotName = name
	})

	dispatch := runtime.ExportedGetReducerDispatch("test_string")
	require.NotNil(t, dispatch)

	w := bsatn.NewWriter(64)
	w.PutString("hello world")

	err := dispatch(newMockCtx(), w.Bytes())
	require.NoError(t, err)
	assert.Equal(t, "hello world", gotName)
}

// --- Test: buildParamDecoder for various types ---

func TestBuildParamDecoderU32(t *testing.T) {
	w := bsatn.NewWriter(4)
	w.PutU32(42)

	r := bsatn.NewReader(w.Bytes())
	var v uint32
	decode := runtime.ExportedBuildParamDecoder(reflect.TypeOf(v))
	err := decode(r, ptrOf(&v))
	require.NoError(t, err)
	assert.Equal(t, uint32(42), v)
}

func TestBuildParamDecoderString(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutString("test")

	r := bsatn.NewReader(w.Bytes())
	var v string
	decode := runtime.ExportedBuildParamDecoder(reflect.TypeOf(v))
	err := decode(r, ptrOf(&v))
	require.NoError(t, err)
	assert.Equal(t, "test", v)
}
