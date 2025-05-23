package reducers

import (
	"fmt"
	"testing"
	"time"
)

func TestWasmRuntimeCreation(t *testing.T) {
	runtime := NewWasmRuntime()

	if runtime == nil {
		t.Fatal("WASM runtime creation failed")
	}

	if runtime.modules == nil {
		t.Error("WASM runtime should initialize modules map")
	}

	if runtime.memoryManager == nil {
		t.Error("WASM runtime should initialize memory manager")
	}

	if runtime.callInterface == nil {
		t.Error("WASM runtime should initialize call interface")
	}

	if runtime.permissions == nil {
		t.Error("WASM runtime should initialize permission manager")
	}
}

func TestWasmModuleLoading(t *testing.T) {
	runtime := NewWasmRuntime()

	// Test module loading
	wasmBytes := []byte{0x00, 0x61, 0x73, 0x6d} // WASM magic bytes
	module, err := runtime.LoadModule("test_module", "Test Module", wasmBytes)

	if err != nil {
		t.Fatalf("Failed to load WASM module: %v", err)
	}

	if module.ID != "test_module" {
		t.Errorf("Expected module ID 'test_module', got '%s'", module.ID)
	}

	if module.Name != "Test Module" {
		t.Errorf("Expected module name 'Test Module', got '%s'", module.Name)
	}

	if !module.JITCompiled {
		t.Error("Module should be JIT compiled with Go 1.24")
	}

	if !module.Active {
		t.Error("Module should be active after loading")
	}

	// Test duplicate loading
	_, err = runtime.LoadModule("test_module", "Duplicate", wasmBytes)
	if err == nil {
		t.Error("Expected error for duplicate module loading")
	}
}

func TestWasmMemoryManagement(t *testing.T) {
	memoryManager := NewWasmMemoryManager()

	// Test memory allocation
	addr, err := memoryManager.Allocate(1024)
	if err != nil {
		t.Fatalf("Failed to allocate memory: %v", err)
	}

	if addr == 0 {
		t.Error("Allocated address should not be zero")
	}

	// Test memory limits
	_, err = memoryManager.Allocate(200 * 1024 * 1024) // Try to allocate 200MB
	if err == nil {
		t.Error("Expected error for exceeding memory limit")
	}

	// Test memory freeing
	err = memoryManager.Free(addr)
	if err != nil {
		t.Errorf("Failed to free memory: %v", err)
	}

	// Test freeing invalid address
	err = memoryManager.Free(0xDEADBEEF)
	if err == nil {
		t.Error("Expected error for freeing invalid address")
	}
}

func TestWasmFunctionCalls(t *testing.T) {
	runtime := NewWasmRuntime()

	// Load test module
	wasmBytes := []byte{0x00, 0x61, 0x73, 0x6d}
	module, err := runtime.LoadModule("func_test", "Function Test", wasmBytes)
	if err != nil {
		t.Fatalf("Failed to load module: %v", err)
	}

	// Add test function
	module.Exports["add"] = &WasmFunction{
		Name: "add",
		Signature: &WasmSignature{
			Parameters: []WasmType{WasmTypeI32, WasmTypeI32},
			Results:    []WasmType{WasmTypeI32},
		},
		Handler: func(args []interface{}) ([]interface{}, error) {
			a := args[0].(int32)
			b := args[1].(int32)
			return []interface{}{a + b}, nil
		},
	}

	// Test function call
	result, err := runtime.CallFunction("func_test", "add", []interface{}{int32(5), int32(3)})
	if err != nil {
		t.Fatalf("Function call failed: %v", err)
	}

	if len(result) != 1 || result[0] != int32(8) {
		t.Errorf("Expected result [8], got %v", result)
	}

	// Test call statistics
	if module.CallCount != 1 {
		t.Errorf("Expected 1 call, got %d", module.CallCount)
	}
}

func TestSpacetimeDBImports(t *testing.T) {
	runtime := NewWasmRuntime()

	// Load module which should have SpacetimeDB imports
	wasmBytes := []byte{0x00, 0x61, 0x73, 0x6d}
	module, err := runtime.LoadModule("spacetime_test", "SpacetimeDB Test", wasmBytes)
	if err != nil {
		t.Fatalf("Failed to load module: %v", err)
	}

	// Verify SpacetimeDB imports are present
	expectedImports := []string{
		"spacetime_insert",
		"spacetime_update",
		"spacetime_delete",
		"spacetime_emit_event",
		"spacetime_random",
		"spacetime_log",
	}

	for _, importName := range expectedImports {
		if _, exists := module.Imports[importName]; !exists {
			t.Errorf("Missing SpacetimeDB import: %s", importName)
		}
	}

	// Test spacetime_random function
	randomFunc := module.Imports["spacetime_random"]
	result, err := randomFunc.Handler([]interface{}{})
	if err != nil {
		t.Errorf("spacetime_random failed: %v", err)
	}

	if len(result) != 1 {
		t.Errorf("Expected 1 result from spacetime_random, got %d", len(result))
	}
}

func TestWasmPermissions(t *testing.T) {
	permissionManager := NewWasmPermissionManager()

	// Test default permissions (should be empty)
	policy := permissionManager.getPolicy("nonexistent")
	if policy != nil {
		t.Error("Expected nil policy for nonexistent module")
	}

	// Create test policy
	testPolicy := &WasmPolicy{
		ModuleID:          "test_module",
		AllowedMemory:     1024 * 1024, // 1MB
		AllowedCallDepth:  10,
		AllowedFunctions:  []string{"add", "subtract"},
		RestrictedImports: []string{"dangerous_function"},
		TimeoutDuration:   5 * time.Second,
	}

	permissionManager.policies["test_module"] = testPolicy

	// Test policy retrieval
	retrievedPolicy := permissionManager.getPolicy("test_module")
	if retrievedPolicy == nil {
		t.Error("Failed to retrieve policy")
	}

	if retrievedPolicy.AllowedMemory != 1024*1024 {
		t.Errorf("Expected 1MB memory limit, got %d", retrievedPolicy.AllowedMemory)
	}
}

func TestWasmCallStack(t *testing.T) {
	callInterface := NewWasmCallInterface()

	// Test call stack management
	call1 := &WasmCall{
		FunctionName: "func1",
		StartTime:    time.Now(),
		Arguments:    []interface{}{1, 2},
		Module:       "test_module",
	}

	call2 := &WasmCall{
		FunctionName: "func2",
		StartTime:    time.Now(),
		Arguments:    []interface{}{3, 4},
		Module:       "test_module",
	}

	// Push calls
	callInterface.pushCall("test_module", call1)
	callInterface.pushCall("test_module", call2)

	// Verify call stack
	if len(callInterface.callStacks["test_module"]) != 2 {
		t.Errorf("Expected 2 calls in stack, got %d", len(callInterface.callStacks["test_module"]))
	}

	// Pop calls
	callInterface.popCall("test_module")
	if len(callInterface.callStacks["test_module"]) != 1 {
		t.Errorf("Expected 1 call after pop, got %d", len(callInterface.callStacks["test_module"]))
	}

	callInterface.popCall("test_module")
	if len(callInterface.callStacks["test_module"]) != 0 {
		t.Errorf("Expected 0 calls after second pop, got %d", len(callInterface.callStacks["test_module"]))
	}
}

func TestWasmRuntimeStats(t *testing.T) {
	runtime := NewWasmRuntime()

	// Load multiple modules
	for i := 0; i < 3; i++ {
		moduleName := fmt.Sprintf("test_module_%d", i)
		wasmBytes := []byte{0x00, 0x61, 0x73, 0x6d}
		module, err := runtime.LoadModule(moduleName, "Test Module", wasmBytes)
		if err != nil {
			t.Fatalf("Failed to load module %s: %v", moduleName, err)
		}

		// Add test function and simulate calls
		module.Exports["test_func"] = &WasmFunction{
			Name: "test_func",
			Signature: &WasmSignature{
				Parameters: []WasmType{},
				Results:    []WasmType{WasmTypeI32},
			},
			Handler: func(args []interface{}) ([]interface{}, error) {
				return []interface{}{int32(42)}, nil
			},
		}

		// Execute function multiple times
		for j := 0; j < 5; j++ {
			runtime.CallFunction(moduleName, "test_func", []interface{}{})
		}
	}

	// Check statistics
	stats := runtime.GetStats()

	if stats.LoadedModules != 3 {
		t.Errorf("Expected 3 loaded modules, got %d", stats.LoadedModules)
	}

	if stats.TotalCalls != 15 { // 3 modules × 5 calls each
		t.Errorf("Expected 15 total calls, got %d", stats.TotalCalls)
	}

	if stats.TotalMemory <= 0 {
		t.Error("Expected positive total memory usage")
	}
}

func BenchmarkWasmFunctionCall(b *testing.B) {
	runtime := NewWasmRuntime()

	// Setup test module
	wasmBytes := []byte{0x00, 0x61, 0x73, 0x6d}
	module, err := runtime.LoadModule("bench_module", "Benchmark Module", wasmBytes)
	if err != nil {
		b.Fatalf("Failed to load module: %v", err)
	}

	module.Exports["fast_func"] = &WasmFunction{
		Name: "fast_func",
		Signature: &WasmSignature{
			Parameters: []WasmType{WasmTypeI32},
			Results:    []WasmType{WasmTypeI32},
		},
		Handler: func(args []interface{}) ([]interface{}, error) {
			return []interface{}{args[0].(int32) * 2}, nil
		},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		runtime.CallFunction("bench_module", "fast_func", []interface{}{int32(i)})
	}
}

func BenchmarkWasmMemoryAllocation(b *testing.B) {
	memoryManager := NewWasmMemoryManager()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		addr, err := memoryManager.Allocate(1024)
		if err != nil {
			b.Fatalf("Allocation failed: %v", err)
		}
		memoryManager.Free(addr)
	}
}
