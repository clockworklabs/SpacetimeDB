package reducers

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb/types"
)

// SpacetimeDB Reducer Framework
// This provides the core reducer functionality that all Go games need

// LifecycleType represents the type of lifecycle event
type LifecycleType int

const (
	// LifecycleInit is called when the module is first initialized
	LifecycleInit LifecycleType = iota
	// LifecycleUpdate is called on every scheduled update
	LifecycleUpdate
	// LifecycleConnect is called when a client connects
	LifecycleConnect
	// LifecycleDisconnect is called when a client disconnects
	LifecycleDisconnect
)

// String returns a string representation of the LifecycleType
func (l LifecycleType) String() string {
	switch l {
	case LifecycleInit:
		return "Init"
	case LifecycleUpdate:
		return "Update"
	case LifecycleConnect:
		return "Connect"
	case LifecycleDisconnect:
		return "Disconnect"
	default:
		return fmt.Sprintf("Unknown(%d)", int(l))
	}
}

// ReducerContext contains information about the current reducer execution
type ReducerContext struct {
	// Sender is the identity of the client that called this reducer
	Sender types.Identity `json:"sender"`

	// Timestamp is when this reducer was called
	Timestamp types.Timestamp `json:"timestamp"`

	// Random seed for deterministic randomness
	RandomSeed uint64 `json:"random_seed"`

	// Context for cancellation and timeouts
	Context context.Context `json:"-"`
}

// String returns a string representation of the ReducerContext
func (r *ReducerContext) String() string {
	return fmt.Sprintf("ReducerContext{Sender: %s, Timestamp: %s, RandomSeed: %d}",
		r.Sender.String(), r.Timestamp.String(), r.RandomSeed)
}

// ReducerResult represents the result of a reducer execution
type ReducerResult struct {
	Success bool   `json:"success"`
	Message string `json:"message,omitempty"`
	Error   error  `json:"-"`
}

// NewSuccessResult creates a successful reducer result
func NewSuccessResult() ReducerResult {
	return ReducerResult{Success: true}
}

// NewSuccessResultWithMessage creates a successful reducer result with a message
func NewSuccessResultWithMessage(message string) ReducerResult {
	return ReducerResult{Success: true, Message: message}
}

// NewErrorResult creates a failed reducer result
func NewErrorResult(err error) ReducerResult {
	return ReducerResult{
		Success: false,
		Message: err.Error(),
		Error:   err,
	}
}

// NewErrorResultWithMessage creates a failed reducer result with a custom message
func NewErrorResultWithMessage(message string) ReducerResult {
	return ReducerResult{
		Success: false,
		Message: message,
		Error:   fmt.Errorf(message),
	}
}

// String returns a string representation of the ReducerResult
func (r ReducerResult) String() string {
	if r.Success {
		if r.Message != "" {
			return fmt.Sprintf("Success: %s", r.Message)
		}
		return "Success"
	}
	return fmt.Sprintf("Error: %s", r.Message)
}

// ReducerFunction represents a function that can be called as a reducer
type ReducerFunction interface {
	// Call executes the reducer with the given context and arguments
	Call(ctx *ReducerContext, args []byte) ReducerResult

	// Name returns the name of this reducer
	Name() string

	// Description returns a description of what this reducer does
	Description() string

	// ArgumentsSchema returns the JSON schema for the reducer arguments (optional)
	ArgumentsSchema() string
}

// LifecycleFunction represents a function that can be called for lifecycle events
type LifecycleFunction interface {
	// Call executes the lifecycle function with the given context and event type
	Call(ctx *ReducerContext, eventType LifecycleType) ReducerResult

	// Name returns the name of this lifecycle function
	Name() string

	// Description returns a description of what this lifecycle function does
	Description() string

	// HandledEvents returns the lifecycle events this function handles
	HandledEvents() []LifecycleType
}

// GenericReducer provides a simple implementation of ReducerFunction
type GenericReducer struct {
	NameStr     string                                               `json:"name"`
	DescStr     string                                               `json:"description"`
	ArgsSchema  string                                               `json:"arguments_schema,omitempty"`
	HandlerFunc func(ctx *ReducerContext, args []byte) ReducerResult `json:"-"`
}

// NewGenericReducer creates a new GenericReducer
func NewGenericReducer(name, description string, handler func(ctx *ReducerContext, args []byte) ReducerResult) *GenericReducer {
	return &GenericReducer{
		NameStr:     name,
		DescStr:     description,
		HandlerFunc: handler,
	}
}

// Call executes the reducer
func (g *GenericReducer) Call(ctx *ReducerContext, args []byte) ReducerResult {
	if g.HandlerFunc == nil {
		return NewErrorResultWithMessage("reducer handler not implemented")
	}
	return g.HandlerFunc(ctx, args)
}

// Name returns the reducer name
func (g *GenericReducer) Name() string {
	return g.NameStr
}

// Description returns the reducer description
func (g *GenericReducer) Description() string {
	return g.DescStr
}

// ArgumentsSchema returns the JSON schema for arguments
func (g *GenericReducer) ArgumentsSchema() string {
	return g.ArgsSchema
}

// SetArgumentsSchema sets the JSON schema for arguments
func (g *GenericReducer) SetArgumentsSchema(schema string) {
	g.ArgsSchema = schema
}

// GenericLifecycleFunction provides a simple implementation of LifecycleFunction
type GenericLifecycleFunction struct {
	NameStr     string                                                           `json:"name"`
	DescStr     string                                                           `json:"description"`
	Events      []LifecycleType                                                  `json:"events"`
	HandlerFunc func(ctx *ReducerContext, eventType LifecycleType) ReducerResult `json:"-"`
}

// NewGenericLifecycleFunction creates a new GenericLifecycleFunction
func NewGenericLifecycleFunction(name, description string, events []LifecycleType, handler func(ctx *ReducerContext, eventType LifecycleType) ReducerResult) *GenericLifecycleFunction {
	return &GenericLifecycleFunction{
		NameStr:     name,
		DescStr:     description,
		Events:      events,
		HandlerFunc: handler,
	}
}

// Call executes the lifecycle function
func (g *GenericLifecycleFunction) Call(ctx *ReducerContext, eventType LifecycleType) ReducerResult {
	if g.HandlerFunc == nil {
		return NewErrorResultWithMessage("lifecycle handler not implemented")
	}
	return g.HandlerFunc(ctx, eventType)
}

// Name returns the function name
func (g *GenericLifecycleFunction) Name() string {
	return g.NameStr
}

// Description returns the function description
func (g *GenericLifecycleFunction) Description() string {
	return g.DescStr
}

// HandledEvents returns the lifecycle events this function handles
func (g *GenericLifecycleFunction) HandledEvents() []LifecycleType {
	return g.Events
}

// ReducerRegistry manages all registered reducers and lifecycle functions
type ReducerRegistry struct {
	reducers        map[string]ReducerFunction   `json:"reducers"`
	lifecycleFuncs  map[string]LifecycleFunction `json:"lifecycle_functions"`
	reducersByID    map[uint32]ReducerFunction   `json:"-"`
	lifecyclesByID  map[uint32]LifecycleFunction `json:"-"`
	nextReducerID   uint32                       `json:"next_reducer_id"`
	nextLifecycleID uint32                       `json:"next_lifecycle_id"`
	mutex           sync.RWMutex                 `json:"-"`
}

// NewReducerRegistry creates a new ReducerRegistry
func NewReducerRegistry() *ReducerRegistry {
	return &ReducerRegistry{
		reducers:        make(map[string]ReducerFunction),
		lifecycleFuncs:  make(map[string]LifecycleFunction),
		reducersByID:    make(map[uint32]ReducerFunction),
		lifecyclesByID:  make(map[uint32]LifecycleFunction),
		nextReducerID:   1,      // Start from 1, reserve 0 for "no reducer"
		nextLifecycleID: 100000, // Start lifecycle IDs from 100000 to avoid collisions
	}
}

// RegisterReducer registers a reducer function
func (r *ReducerRegistry) RegisterReducer(reducer ReducerFunction) uint32 {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	id := r.nextReducerID
	r.nextReducerID++

	r.reducers[reducer.Name()] = reducer
	r.reducersByID[id] = reducer

	return id
}

// RegisterLifecycleFunction registers a lifecycle function
func (r *ReducerRegistry) RegisterLifecycleFunction(lifecycle LifecycleFunction) uint32 {
	r.mutex.Lock()
	defer r.mutex.Unlock()

	id := r.nextLifecycleID
	r.nextLifecycleID++

	r.lifecycleFuncs[lifecycle.Name()] = lifecycle
	r.lifecyclesByID[id] = lifecycle

	return id
}

// GetReducer returns a reducer by name
func (r *ReducerRegistry) GetReducer(name string) (ReducerFunction, bool) {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	reducer, exists := r.reducers[name]
	return reducer, exists
}

// GetReducerByID returns a reducer by ID
func (r *ReducerRegistry) GetReducerByID(id uint32) (ReducerFunction, bool) {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	reducer, exists := r.reducersByID[id]
	return reducer, exists
}

// GetLifecycleFunction returns a lifecycle function by name
func (r *ReducerRegistry) GetLifecycleFunction(name string) (LifecycleFunction, bool) {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	lifecycle, exists := r.lifecycleFuncs[name]
	return lifecycle, exists
}

// GetLifecycleFunctionByID returns a lifecycle function by ID
func (r *ReducerRegistry) GetLifecycleFunctionByID(id uint32) (LifecycleFunction, bool) {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	lifecycle, exists := r.lifecyclesByID[id]
	return lifecycle, exists
}

// GetAllReducers returns all registered reducers
func (r *ReducerRegistry) GetAllReducers() map[string]ReducerFunction {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	result := make(map[string]ReducerFunction)
	for k, v := range r.reducers {
		result[k] = v
	}
	return result
}

// GetAllLifecycleFunctions returns all registered lifecycle functions
func (r *ReducerRegistry) GetAllLifecycleFunctions() map[string]LifecycleFunction {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	result := make(map[string]LifecycleFunction)
	for k, v := range r.lifecycleFuncs {
		result[k] = v
	}
	return result
}

// ReducerCount returns the number of registered reducers
func (r *ReducerRegistry) ReducerCount() int {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	return len(r.reducers)
}

// LifecycleFunctionCount returns the number of registered lifecycle functions
func (r *ReducerRegistry) LifecycleFunctionCount() int {
	r.mutex.RLock()
	defer r.mutex.RUnlock()
	return len(r.lifecycleFuncs)
}

// GetByID returns either a reducer or lifecycle function by ID
func (r *ReducerRegistry) GetByID(id uint32) (interface{}, bool) {
	r.mutex.RLock()
	defer r.mutex.RUnlock()

	if reducer, exists := r.reducersByID[id]; exists {
		return reducer, true
	}
	if lifecycle, exists := r.lifecyclesByID[id]; exists {
		return lifecycle, true
	}
	return nil, false
}

// Stats returns statistics about the registry
func (r *ReducerRegistry) Stats() map[string]interface{} {
	r.mutex.RLock()
	defer r.mutex.RUnlock()

	return map[string]interface{}{
		"reducer_count":            len(r.reducers),
		"lifecycle_function_count": len(r.lifecycleFuncs),
		"next_reducer_id":          r.nextReducerID,
		"next_lifecycle_id":        r.nextLifecycleID,
	}
}

// ToJSON returns a JSON representation of the registry
func (r *ReducerRegistry) ToJSON() ([]byte, error) {
	r.mutex.RLock()
	defer r.mutex.RUnlock()

	data := map[string]interface{}{
		"reducers": func() map[string]interface{} {
			result := make(map[string]interface{})
			for name, reducer := range r.reducers {
				result[name] = map[string]interface{}{
					"name":        reducer.Name(),
					"description": reducer.Description(),
					"schema":      reducer.ArgumentsSchema(),
				}
			}
			return result
		}(),
		"lifecycle_functions": func() map[string]interface{} {
			result := make(map[string]interface{})
			for name, lifecycle := range r.lifecycleFuncs {
				result[name] = map[string]interface{}{
					"name":        lifecycle.Name(),
					"description": lifecycle.Description(),
					"events":      lifecycle.HandledEvents(),
				}
			}
			return result
		}(),
		"stats": r.Stats(),
	}

	return json.MarshalIndent(data, "", "  ")
}

// Performance monitoring utilities

// ReducerMetrics tracks performance metrics for reducers
type ReducerMetrics struct {
	Name            string        `json:"name"`
	CallCount       uint64        `json:"call_count"`
	TotalDuration   time.Duration `json:"total_duration"`
	AverageDuration time.Duration `json:"average_duration"`
	LastCall        time.Time     `json:"last_call"`
	ErrorCount      uint64        `json:"error_count"`
	mutex           sync.RWMutex  `json:"-"`
}

// NewReducerMetrics creates a new ReducerMetrics
func NewReducerMetrics(name string) *ReducerMetrics {
	return &ReducerMetrics{
		Name: name,
	}
}

// RecordCall records a successful call
func (m *ReducerMetrics) RecordCall(duration time.Duration) {
	m.mutex.Lock()
	defer m.mutex.Unlock()

	m.CallCount++
	m.TotalDuration += duration
	m.AverageDuration = m.TotalDuration / time.Duration(m.CallCount)
	m.LastCall = time.Now()
}

// RecordError records an error
func (m *ReducerMetrics) RecordError(duration time.Duration) {
	m.mutex.Lock()
	defer m.mutex.Unlock()

	m.CallCount++
	m.ErrorCount++
	m.TotalDuration += duration
	m.AverageDuration = m.TotalDuration / time.Duration(m.CallCount)
	m.LastCall = time.Now()
}

// GetStats returns current statistics
func (m *ReducerMetrics) GetStats() map[string]interface{} {
	m.mutex.RLock()
	defer m.mutex.RUnlock()

	var errorRate float64
	if m.CallCount > 0 {
		errorRate = float64(m.ErrorCount) / float64(m.CallCount) * 100.0
	}

	return map[string]interface{}{
		"name":             m.Name,
		"call_count":       m.CallCount,
		"total_duration":   m.TotalDuration.String(),
		"average_duration": m.AverageDuration.String(),
		"last_call":        m.LastCall.Format(time.RFC3339),
		"error_count":      m.ErrorCount,
		"error_rate":       fmt.Sprintf("%.2f%%", errorRate),
	}
}

// Global registry instance (can be overridden by applications)
var DefaultRegistry = NewReducerRegistry()

// Global convenience functions that use the default registry

// RegisterReducer registers a reducer with the default registry
func RegisterReducer(reducer ReducerFunction) uint32 {
	return DefaultRegistry.RegisterReducer(reducer)
}

// RegisterLifecycleFunction registers a lifecycle function with the default registry
func RegisterLifecycleFunction(lifecycle LifecycleFunction) uint32 {
	return DefaultRegistry.RegisterLifecycleFunction(lifecycle)
}

// GetReducer gets a reducer from the default registry
func GetReducer(name string) (ReducerFunction, bool) {
	return DefaultRegistry.GetReducer(name)
}

// GetLifecycleFunction gets a lifecycle function from the default registry
func GetLifecycleFunction(name string) (LifecycleFunction, bool) {
	return DefaultRegistry.GetLifecycleFunction(name)
}
