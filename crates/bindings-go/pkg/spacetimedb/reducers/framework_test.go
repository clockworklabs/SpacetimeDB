package reducers

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/types"
)

func TestLifecycleType(t *testing.T) {
	tests := []struct {
		lifecycle LifecycleType
		expected  string
	}{
		{LifecycleInit, "Init"},
		{LifecycleUpdate, "Update"},
		{LifecycleConnect, "Connect"},
		{LifecycleDisconnect, "Disconnect"},
		{LifecycleType(999), "Unknown(999)"},
	}

	for _, test := range tests {
		if got := test.lifecycle.String(); got != test.expected {
			t.Errorf("LifecycleType(%d).String() = %q, want %q", test.lifecycle, got, test.expected)
		}
	}
}

func TestReducerContext(t *testing.T) {
	identity := types.NewIdentity([16]byte{0x01, 0x02, 0x03, 0x04})
	timestamp := types.NewTimestamp(1234567890)
	ctx := context.Background()

	reducerCtx := &ReducerContext{
		Sender:     identity,
		Timestamp:  timestamp,
		RandomSeed: 42,
		Context:    ctx,
	}

	t.Run("String", func(t *testing.T) {
		str := reducerCtx.String()
		if str == "" {
			t.Error("String() should not be empty")
		}
	})
}

func TestReducerResult(t *testing.T) {
	t.Run("NewSuccessResult", func(t *testing.T) {
		result := NewSuccessResult()
		if !result.Success {
			t.Error("NewSuccessResult() should create successful result")
		}
		if result.Message != "" {
			t.Error("NewSuccessResult() should have empty message")
		}
		if result.Error != nil {
			t.Error("NewSuccessResult() should have nil error")
		}
	})

	t.Run("NewSuccessResultWithMessage", func(t *testing.T) {
		message := "Operation completed"
		result := NewSuccessResultWithMessage(message)
		if !result.Success {
			t.Error("NewSuccessResultWithMessage() should create successful result")
		}
		if result.Message != message {
			t.Errorf("NewSuccessResultWithMessage() message = %q, want %q", result.Message, message)
		}
	})

	t.Run("NewErrorResult", func(t *testing.T) {
		err := errors.New("test error")
		result := NewErrorResult(err)
		if result.Success {
			t.Error("NewErrorResult() should create failed result")
		}
		if result.Message != err.Error() {
			t.Errorf("NewErrorResult() message = %q, want %q", result.Message, err.Error())
		}
		if result.Error != err {
			t.Error("NewErrorResult() should preserve original error")
		}
	})

	t.Run("NewErrorResultWithMessage", func(t *testing.T) {
		message := "Custom error"
		result := NewErrorResultWithMessage(message)
		if result.Success {
			t.Error("NewErrorResultWithMessage() should create failed result")
		}
		if result.Message != message {
			t.Errorf("NewErrorResultWithMessage() message = %q, want %q", result.Message, message)
		}
		if result.Error == nil {
			t.Error("NewErrorResultWithMessage() should create error")
		}
	})

	t.Run("String", func(t *testing.T) {
		successResult := NewSuccessResult()
		if successResult.String() != "Success" {
			t.Errorf("Success result String() = %q, want %q", successResult.String(), "Success")
		}

		successWithMessage := NewSuccessResultWithMessage("test")
		if successWithMessage.String() != "Success: test" {
			t.Errorf("Success with message String() = %q, want %q", successWithMessage.String(), "Success: test")
		}

		errorResult := NewErrorResultWithMessage("test error")
		if errorResult.String() != "Error: test error" {
			t.Errorf("Error result String() = %q, want %q", errorResult.String(), "Error: test error")
		}
	})
}

func TestGenericReducer(t *testing.T) {
	t.Run("Creation", func(t *testing.T) {
		name := "test_reducer"
		description := "A test reducer"
		handler := func(ctx *ReducerContext, args []byte) ReducerResult {
			return NewSuccessResult()
		}

		reducer := NewGenericReducer(name, description, handler)
		if reducer.Name() != name {
			t.Errorf("Name() = %q, want %q", reducer.Name(), name)
		}
		if reducer.Description() != description {
			t.Errorf("Description() = %q, want %q", reducer.Description(), description)
		}
		if reducer.ArgumentsSchema() != "" {
			t.Error("ArgumentsSchema() should be empty initially")
		}
	})

	t.Run("Call", func(t *testing.T) {
		called := false
		handler := func(ctx *ReducerContext, args []byte) ReducerResult {
			called = true
			return NewSuccessResult()
		}

		reducer := NewGenericReducer("test", "test", handler)
		ctx := &ReducerContext{}
		result := reducer.Call(ctx, []byte{})

		if !called {
			t.Error("Handler should have been called")
		}
		if !result.Success {
			t.Error("Result should be successful")
		}
	})

	t.Run("CallWithoutHandler", func(t *testing.T) {
		reducer := &GenericReducer{
			NameStr: "test",
			DescStr: "test",
		}
		ctx := &ReducerContext{}
		result := reducer.Call(ctx, []byte{})

		if result.Success {
			t.Error("Result should be failed when no handler")
		}
	})

	t.Run("SetArgumentsSchema", func(t *testing.T) {
		reducer := NewGenericReducer("test", "test", nil)
		schema := `{"type": "object"}`
		reducer.SetArgumentsSchema(schema)
		if reducer.ArgumentsSchema() != schema {
			t.Errorf("ArgumentsSchema() = %q, want %q", reducer.ArgumentsSchema(), schema)
		}
	})
}

func TestGenericLifecycleFunction(t *testing.T) {
	t.Run("Creation", func(t *testing.T) {
		name := "test_lifecycle"
		description := "A test lifecycle function"
		events := []LifecycleType{LifecycleInit, LifecycleConnect}
		handler := func(ctx *ReducerContext, eventType LifecycleType) ReducerResult {
			return NewSuccessResult()
		}

		lifecycle := NewGenericLifecycleFunction(name, description, events, handler)
		if lifecycle.Name() != name {
			t.Errorf("Name() = %q, want %q", lifecycle.Name(), name)
		}
		if lifecycle.Description() != description {
			t.Errorf("Description() = %q, want %q", lifecycle.Description(), description)
		}
		handledEvents := lifecycle.HandledEvents()
		if len(handledEvents) != len(events) {
			t.Errorf("HandledEvents() length = %d, want %d", len(handledEvents), len(events))
		}
	})

	t.Run("Call", func(t *testing.T) {
		called := false
		var receivedEventType LifecycleType
		handler := func(ctx *ReducerContext, eventType LifecycleType) ReducerResult {
			called = true
			receivedEventType = eventType
			return NewSuccessResult()
		}

		lifecycle := NewGenericLifecycleFunction("test", "test", []LifecycleType{LifecycleInit}, handler)
		ctx := &ReducerContext{}
		result := lifecycle.Call(ctx, LifecycleInit)

		if !called {
			t.Error("Handler should have been called")
		}
		if receivedEventType != LifecycleInit {
			t.Errorf("Received event type = %v, want %v", receivedEventType, LifecycleInit)
		}
		if !result.Success {
			t.Error("Result should be successful")
		}
	})

	t.Run("CallWithoutHandler", func(t *testing.T) {
		lifecycle := &GenericLifecycleFunction{
			NameStr: "test",
			DescStr: "test",
			Events:  []LifecycleType{LifecycleInit},
		}
		ctx := &ReducerContext{}
		result := lifecycle.Call(ctx, LifecycleInit)

		if result.Success {
			t.Error("Result should be failed when no handler")
		}
	})
}

func TestReducerRegistry(t *testing.T) {
	t.Run("Creation", func(t *testing.T) {
		registry := NewReducerRegistry()
		if registry.ReducerCount() != 0 {
			t.Error("New registry should have 0 reducers")
		}
		if registry.LifecycleFunctionCount() != 0 {
			t.Error("New registry should have 0 lifecycle functions")
		}
	})

	t.Run("RegisterReducer", func(t *testing.T) {
		registry := NewReducerRegistry()
		reducer := NewGenericReducer("test", "test", nil)

		id := registry.RegisterReducer(reducer)
		if id == 0 {
			t.Error("RegisterReducer should return non-zero ID")
		}
		if registry.ReducerCount() != 1 {
			t.Error("Registry should have 1 reducer after registration")
		}

		// Test retrieval by name
		retrieved, exists := registry.GetReducer("test")
		if !exists {
			t.Error("GetReducer should find registered reducer")
		}
		if retrieved.Name() != "test" {
			t.Error("Retrieved reducer should have correct name")
		}

		// Test retrieval by ID
		retrievedByID, exists := registry.GetReducerByID(id)
		if !exists {
			t.Error("GetReducerByID should find registered reducer")
		}
		if retrievedByID.Name() != "test" {
			t.Error("Retrieved reducer by ID should have correct name")
		}
	})

	t.Run("RegisterLifecycleFunction", func(t *testing.T) {
		registry := NewReducerRegistry()
		lifecycle := NewGenericLifecycleFunction("test", "test", []LifecycleType{LifecycleInit}, nil)

		id := registry.RegisterLifecycleFunction(lifecycle)
		if id == 0 {
			t.Error("RegisterLifecycleFunction should return non-zero ID")
		}
		if registry.LifecycleFunctionCount() != 1 {
			t.Error("Registry should have 1 lifecycle function after registration")
		}

		// Test retrieval by name
		retrieved, exists := registry.GetLifecycleFunction("test")
		if !exists {
			t.Error("GetLifecycleFunction should find registered function")
		}
		if retrieved.Name() != "test" {
			t.Error("Retrieved lifecycle function should have correct name")
		}

		// Test retrieval by ID
		retrievedByID, exists := registry.GetLifecycleFunctionByID(id)
		if !exists {
			t.Error("GetLifecycleFunctionByID should find registered function")
		}
		if retrievedByID.Name() != "test" {
			t.Error("Retrieved lifecycle function by ID should have correct name")
		}
	})

	t.Run("GetByID", func(t *testing.T) {
		registry := NewReducerRegistry()
		reducer := NewGenericReducer("test_reducer", "test", nil)
		lifecycle := NewGenericLifecycleFunction("test_lifecycle", "test", []LifecycleType{LifecycleInit}, nil)

		reducerID := registry.RegisterReducer(reducer)
		lifecycleID := registry.RegisterLifecycleFunction(lifecycle)

		// Test getting reducer by ID
		retrieved, exists := registry.GetByID(reducerID)
		if !exists {
			t.Error("GetByID should find registered reducer")
		}
		if retrievedReducer, ok := retrieved.(ReducerFunction); ok {
			if retrievedReducer.Name() != "test_reducer" {
				t.Error("Retrieved reducer should have correct name")
			}
		} else {
			t.Error("Retrieved item should be a ReducerFunction")
		}

		// Test getting lifecycle function by ID
		retrieved, exists = registry.GetByID(lifecycleID)
		if !exists {
			t.Error("GetByID should find registered lifecycle function")
		}
		if retrievedLifecycle, ok := retrieved.(LifecycleFunction); ok {
			if retrievedLifecycle.Name() != "test_lifecycle" {
				t.Error("Retrieved lifecycle function should have correct name")
			}
		} else {
			t.Errorf("Retrieved item should be a LifecycleFunction, got %T", retrieved)
		}

		// Test non-existent ID
		_, exists = registry.GetByID(999)
		if exists {
			t.Error("GetByID should not find non-existent ID")
		}
	})

	t.Run("GetAllReducers", func(t *testing.T) {
		registry := NewReducerRegistry()
		reducer1 := NewGenericReducer("test1", "test", nil)
		reducer2 := NewGenericReducer("test2", "test", nil)

		registry.RegisterReducer(reducer1)
		registry.RegisterReducer(reducer2)

		all := registry.GetAllReducers()
		if len(all) != 2 {
			t.Errorf("GetAllReducers() length = %d, want 2", len(all))
		}
		if _, exists := all["test1"]; !exists {
			t.Error("GetAllReducers() should include test1")
		}
		if _, exists := all["test2"]; !exists {
			t.Error("GetAllReducers() should include test2")
		}
	})

	t.Run("GetAllLifecycleFunctions", func(t *testing.T) {
		registry := NewReducerRegistry()
		lifecycle1 := NewGenericLifecycleFunction("test1", "test", []LifecycleType{LifecycleInit}, nil)
		lifecycle2 := NewGenericLifecycleFunction("test2", "test", []LifecycleType{LifecycleConnect}, nil)

		registry.RegisterLifecycleFunction(lifecycle1)
		registry.RegisterLifecycleFunction(lifecycle2)

		all := registry.GetAllLifecycleFunctions()
		if len(all) != 2 {
			t.Errorf("GetAllLifecycleFunctions() length = %d, want 2", len(all))
		}
		if _, exists := all["test1"]; !exists {
			t.Error("GetAllLifecycleFunctions() should include test1")
		}
		if _, exists := all["test2"]; !exists {
			t.Error("GetAllLifecycleFunctions() should include test2")
		}
	})

	t.Run("Stats", func(t *testing.T) {
		registry := NewReducerRegistry()
		reducer := NewGenericReducer("test_reducer", "test", nil)
		lifecycle := NewGenericLifecycleFunction("test_lifecycle", "test", []LifecycleType{LifecycleInit}, nil)

		registry.RegisterReducer(reducer)
		registry.RegisterLifecycleFunction(lifecycle)

		stats := registry.Stats()
		if stats["reducer_count"] != 1 {
			t.Errorf("Stats reducer_count = %v, want 1", stats["reducer_count"])
		}
		if stats["lifecycle_function_count"] != 1 {
			t.Errorf("Stats lifecycle_function_count = %v, want 1", stats["lifecycle_function_count"])
		}
	})

	t.Run("ToJSON", func(t *testing.T) {
		registry := NewReducerRegistry()
		reducer := NewGenericReducer("test_reducer", "A test reducer", nil)
		reducer.SetArgumentsSchema(`{"type": "object"}`)
		lifecycle := NewGenericLifecycleFunction("test_lifecycle", "A test lifecycle", []LifecycleType{LifecycleInit}, nil)

		registry.RegisterReducer(reducer)
		registry.RegisterLifecycleFunction(lifecycle)

		data, err := registry.ToJSON()
		if err != nil {
			t.Fatalf("ToJSON() error = %v", err)
		}

		var result map[string]interface{}
		err = json.Unmarshal(data, &result)
		if err != nil {
			t.Fatalf("JSON unmarshal error = %v", err)
		}

		// Check structure
		if _, exists := result["reducers"]; !exists {
			t.Error("JSON should include reducers")
		}
		if _, exists := result["lifecycle_functions"]; !exists {
			t.Error("JSON should include lifecycle_functions")
		}
		if _, exists := result["stats"]; !exists {
			t.Error("JSON should include stats")
		}
	})
}

func TestReducerMetrics(t *testing.T) {
	t.Run("Creation", func(t *testing.T) {
		metrics := NewReducerMetrics("test")
		if metrics.Name != "test" {
			t.Errorf("Name = %q, want %q", metrics.Name, "test")
		}
		if metrics.CallCount != 0 {
			t.Error("CallCount should be 0 initially")
		}
		if metrics.ErrorCount != 0 {
			t.Error("ErrorCount should be 0 initially")
		}
	})

	t.Run("RecordCall", func(t *testing.T) {
		metrics := NewReducerMetrics("test")
		duration := 100 * time.Millisecond

		metrics.RecordCall(duration)

		if metrics.CallCount != 1 {
			t.Errorf("CallCount = %d, want 1", metrics.CallCount)
		}
		if metrics.TotalDuration != duration {
			t.Errorf("TotalDuration = %v, want %v", metrics.TotalDuration, duration)
		}
		if metrics.AverageDuration != duration {
			t.Errorf("AverageDuration = %v, want %v", metrics.AverageDuration, duration)
		}
		if metrics.ErrorCount != 0 {
			t.Error("ErrorCount should still be 0")
		}
	})

	t.Run("RecordError", func(t *testing.T) {
		metrics := NewReducerMetrics("test")
		duration := 50 * time.Millisecond

		metrics.RecordError(duration)

		if metrics.CallCount != 1 {
			t.Errorf("CallCount = %d, want 1", metrics.CallCount)
		}
		if metrics.ErrorCount != 1 {
			t.Errorf("ErrorCount = %d, want 1", metrics.ErrorCount)
		}
		if metrics.TotalDuration != duration {
			t.Errorf("TotalDuration = %v, want %v", metrics.TotalDuration, duration)
		}
	})

	t.Run("GetStats", func(t *testing.T) {
		metrics := NewReducerMetrics("test")

		// Record some calls
		metrics.RecordCall(100 * time.Millisecond)
		metrics.RecordCall(200 * time.Millisecond)
		metrics.RecordError(50 * time.Millisecond)

		stats := metrics.GetStats()

		if stats["name"] != "test" {
			t.Errorf("Stats name = %v, want %q", stats["name"], "test")
		}
		if stats["call_count"] != uint64(3) {
			t.Errorf("Stats call_count = %v, want 3", stats["call_count"])
		}
		if stats["error_count"] != uint64(1) {
			t.Errorf("Stats error_count = %v, want 1", stats["error_count"])
		}

		errorRate := stats["error_rate"].(string)
		if errorRate != "33.33%" {
			t.Errorf("Stats error_rate = %v, want %q", errorRate, "33.33%")
		}
	})
}

func TestGlobalFunctions(t *testing.T) {
	// Save original registry
	originalRegistry := DefaultRegistry
	defer func() {
		DefaultRegistry = originalRegistry
	}()

	// Use a fresh registry for testing
	DefaultRegistry = NewReducerRegistry()

	t.Run("RegisterReducer", func(t *testing.T) {
		reducer := NewGenericReducer("global_test", "test", nil)
		id := RegisterReducer(reducer)
		if id == 0 {
			t.Error("RegisterReducer should return non-zero ID")
		}

		retrieved, exists := GetReducer("global_test")
		if !exists {
			t.Error("GetReducer should find globally registered reducer")
		}
		if retrieved.Name() != "global_test" {
			t.Error("Retrieved reducer should have correct name")
		}
	})

	t.Run("RegisterLifecycleFunction", func(t *testing.T) {
		lifecycle := NewGenericLifecycleFunction("global_lifecycle", "test", []LifecycleType{LifecycleInit}, nil)
		id := RegisterLifecycleFunction(lifecycle)
		if id == 0 {
			t.Error("RegisterLifecycleFunction should return non-zero ID")
		}

		retrieved, exists := GetLifecycleFunction("global_lifecycle")
		if !exists {
			t.Error("GetLifecycleFunction should find globally registered function")
		}
		if retrieved.Name() != "global_lifecycle" {
			t.Error("Retrieved lifecycle function should have correct name")
		}
	})
}

// Benchmark tests
func BenchmarkReducerRegistration(b *testing.B) {
	registry := NewReducerRegistry()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reducer := NewGenericReducer("test", "test", nil)
		registry.RegisterReducer(reducer)
	}
}

func BenchmarkReducerCall(b *testing.B) {
	handler := func(ctx *ReducerContext, args []byte) ReducerResult {
		return NewSuccessResult()
	}
	reducer := NewGenericReducer("test", "test", handler)
	ctx := &ReducerContext{}
	args := []byte("test")

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		reducer.Call(ctx, args)
	}
}

func BenchmarkMetricsRecording(b *testing.B) {
	metrics := NewReducerMetrics("test")
	duration := 100 * time.Millisecond

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if i%10 == 0 {
			metrics.RecordError(duration)
		} else {
			metrics.RecordCall(duration)
		}
	}
}
