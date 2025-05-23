package reducers

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/realtime"
)

func TestReducerRegistry(t *testing.T) {
	registry := NewReducerRegistry()

	// Test registration
	testReducer := NewReducer("test_reducer", ReducerTypeUpdate, func(ctx *ReducerContext, args []interface{}) (interface{}, error) {
		return "test_result", nil
	})

	err := registry.Register(testReducer)
	if err != nil {
		t.Fatalf("Failed to register reducer: %v", err)
	}

	// Test duplicate registration
	err = registry.Register(testReducer)
	if err == nil {
		t.Error("Expected error for duplicate registration")
	}

	// Test retrieval
	retrieved, exists := registry.Get("test_reducer")
	if !exists {
		t.Error("Reducer not found after registration")
	}

	if retrieved.Name() != "test_reducer" {
		t.Errorf("Expected name 'test_reducer', got '%s'", retrieved.Name())
	}
}

func TestReducerExecution(t *testing.T) {
	registry := NewReducerRegistry()

	// Create test reducer
	testReducer := NewReducer("test_exec", ReducerTypeUpdate, func(ctx *ReducerContext, args []interface{}) (interface{}, error) {
		if len(args) != 2 {
			return nil, fmt.Errorf("expected 2 args, got %d", len(args))
		}

		a := args[0].(int)
		b := args[1].(int)
		return a + b, nil
	}).WithParameter("a", reflect.TypeOf(int(0)), true).
		WithParameter("b", reflect.TypeOf(int(0)), true)

	registry.Register(testReducer)

	// Execute reducer
	result, err := registry.Execute(context.Background(), "test_exec", []interface{}{5, 3})
	if err != nil {
		t.Fatalf("Execution failed: %v", err)
	}

	if result != 8 {
		t.Errorf("Expected result 8, got %v", result)
	}
}

func TestWasmIntegration(t *testing.T) {
	wasmCtx := NewWasmContext()

	if wasmCtx.ModuleID == "" {
		t.Error("WASM context should have a module ID")
	}

	if wasmCtx.MemoryAccess.Size != 1024*1024 {
		t.Errorf("Expected 1MB memory, got %d", wasmCtx.MemoryAccess.Size)
	}

	if !wasmCtx.Permissions.CanAccessDatabase {
		t.Error("WASM should have database access permissions")
	}
}

func TestEventIntegration(t *testing.T) {
	registry := NewReducerRegistry()

	// Track events
	var receivedEvents []*realtime.TableEvent
	tableFilter := realtime.TableFilter("test_table")
	registry.eventBus.Subscribe(tableFilter, func(event *realtime.TableEvent) {
		receivedEvents = append(receivedEvents, event)
	})

	// Create reducer that emits event
	eventReducer := NewReducer("event_test", ReducerTypeUpdate, func(ctx *ReducerContext, args []interface{}) (interface{}, error) {
		ctx.Events.PublishEvent(&realtime.TableEvent{
			Type:      realtime.EventInsert,
			TableName: "test_table",
			Entity:    map[string]interface{}{"id": 1},
		})
		return "event_emitted", nil
	})

	registry.Register(eventReducer)

	// Execute and wait for event
	_, err := registry.Execute(context.Background(), "event_test", []interface{}{})
	if err != nil {
		t.Fatalf("Execution failed: %v", err)
	}

	time.Sleep(10 * time.Millisecond)

	if len(receivedEvents) != 1 {
		t.Errorf("Expected 1 event, got %d", len(receivedEvents))
	}
}

func TestReducerStatistics(t *testing.T) {
	registry := NewReducerRegistry()

	testReducer := NewReducer("stats_test", ReducerTypeUpdate, func(ctx *ReducerContext, args []interface{}) (interface{}, error) {
		return "ok", nil
	})

	registry.Register(testReducer)

	// Execute multiple times
	for i := 0; i < 5; i++ {
		registry.Execute(context.Background(), "stats_test", []interface{}{})
	}

	stats := registry.Stats()
	if stats.TotalReducers != 1 {
		t.Errorf("Expected 1 total reducer, got %d", stats.TotalReducers)
	}

	if stats.ExecutionCount != 5 {
		t.Errorf("Expected 5 executions, got %d", stats.ExecutionCount)
	}
}

func BenchmarkReducerExecution(b *testing.B) {
	registry := NewReducerRegistry()

	benchReducer := NewReducer("bench_test", ReducerTypeUpdate, func(ctx *ReducerContext, args []interface{}) (interface{}, error) {
		return "fast_result", nil
	})

	registry.Register(benchReducer)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		registry.Execute(context.Background(), "bench_test", []interface{}{})
	}
}
