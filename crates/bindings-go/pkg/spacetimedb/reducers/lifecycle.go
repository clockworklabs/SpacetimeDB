// Package reducers - Universal lifecycle reducer patterns for SpacetimeDB games
package reducers

import (
	"encoding/json"
	"fmt"
	"time"
)

// LifecycleReducerBuilder provides utilities for building common lifecycle reducers
type LifecycleReducerBuilder struct {
	// Configuration
	playerSessionEnabled bool
	performanceEnabled   bool
	autoScheduleTimers   bool

	// Event handlers
	onInit       func(*ReducerContext) error
	onConnect    func(*ReducerContext, *Identity) error
	onDisconnect func(*ReducerContext, *Identity) error

	// Timer configuration
	gameTickInterval time.Duration
	cleanupInterval  time.Duration

	// Player management
	playerRestorationEnabled bool
}

// NewLifecycleReducerBuilder creates a new lifecycle reducer builder
func NewLifecycleReducerBuilder() *LifecycleReducerBuilder {
	return &LifecycleReducerBuilder{
		playerSessionEnabled:     true,
		performanceEnabled:       true,
		autoScheduleTimers:       true,
		gameTickInterval:         100 * time.Millisecond,
		cleanupInterval:          time.Minute,
		playerRestorationEnabled: true,
	}
}

// WithPlayerSessionManagement enables/disables automatic player session management
func (b *LifecycleReducerBuilder) WithPlayerSessionManagement(enabled bool) *LifecycleReducerBuilder {
	b.playerSessionEnabled = enabled
	return b
}

// WithPerformanceMonitoring enables/disables performance monitoring
func (b *LifecycleReducerBuilder) WithPerformanceMonitoring(enabled bool) *LifecycleReducerBuilder {
	b.performanceEnabled = enabled
	return b
}

// WithAutoScheduleTimers enables/disables automatic timer scheduling
func (b *LifecycleReducerBuilder) WithAutoScheduleTimers(enabled bool) *LifecycleReducerBuilder {
	b.autoScheduleTimers = enabled
	return b
}

// WithGameTickInterval sets the game tick interval
func (b *LifecycleReducerBuilder) WithGameTickInterval(interval time.Duration) *LifecycleReducerBuilder {
	b.gameTickInterval = interval
	return b
}

// WithCleanupInterval sets the cleanup interval
func (b *LifecycleReducerBuilder) WithCleanupInterval(interval time.Duration) *LifecycleReducerBuilder {
	b.cleanupInterval = interval
	return b
}

// WithPlayerRestoration enables/disables player session restoration
func (b *LifecycleReducerBuilder) WithPlayerRestoration(enabled bool) *LifecycleReducerBuilder {
	b.playerRestorationEnabled = enabled
	return b
}

// OnInit sets the init event handler
func (b *LifecycleReducerBuilder) OnInit(handler func(*ReducerContext) error) *LifecycleReducerBuilder {
	b.onInit = handler
	return b
}

// OnConnect sets the connect event handler
func (b *LifecycleReducerBuilder) OnConnect(handler func(*ReducerContext, *Identity) error) *LifecycleReducerBuilder {
	b.onConnect = handler
	return b
}

// OnDisconnect sets the disconnect event handler
func (b *LifecycleReducerBuilder) OnDisconnect(handler func(*ReducerContext, *Identity) error) *LifecycleReducerBuilder {
	b.onDisconnect = handler
	return b
}

// BuildInitReducer creates a universal init reducer
func (b *LifecycleReducerBuilder) BuildInitReducer() *GenericReducer {
	return NewGenericReducer("Init", "Universal init reducer", func(ctx *ReducerContext, args []byte) ReducerResult {
		var timer *PerformanceTimer
		if b.performanceEnabled {
			timer = NewPerformanceTimer("Init")
			defer timer.Stop()
		}

		LogInfo("Initializing SpacetimeDB module...")

		// Call custom init handler if provided
		if b.onInit != nil {
			if err := b.onInit(ctx); err != nil {
				return NewErrorResult(fmt.Errorf("Custom init failed: %v", err))
			}
		}

		// Schedule automatic timers if enabled
		if b.autoScheduleTimers {
			if err := b.scheduleDefaultTimers(ctx); err != nil {
				return NewErrorResult(fmt.Errorf("Failed to schedule timers: %v", err))
			}
		}

		LogInfo("SpacetimeDB module initialized successfully")
		return NewSuccessResult()
	})
}

// BuildConnectReducer creates a universal connect reducer
func (b *LifecycleReducerBuilder) BuildConnectReducer() *GenericReducer {
	return NewGenericReducer("Connect", "Universal connect reducer", func(ctx *ReducerContext, args []byte) ReducerResult {
		var timer *PerformanceTimer
		if b.performanceEnabled {
			timer = NewPerformanceTimer("Connect")
			defer timer.Stop()
		}

		identity := ctx.Sender
		LogInfo(fmt.Sprintf("Client connecting: %s", identity.Name))

		// Handle player session management if enabled
		if b.playerSessionEnabled {
			if err := b.handlePlayerConnection(ctx, identity); err != nil {
				return NewErrorResult(fmt.Errorf("Player connection failed: %v", err))
			}
		}

		// Call custom connect handler if provided
		if b.onConnect != nil {
			if err := b.onConnect(ctx, identity); err != nil {
				return NewErrorResult(fmt.Errorf("Custom connect failed: %v", err))
			}
		}

		LogInfo(fmt.Sprintf("Client connected successfully: %s", identity.Name))
		return NewSuccessResult()
	})
}

// BuildDisconnectReducer creates a universal disconnect reducer
func (b *LifecycleReducerBuilder) BuildDisconnectReducer() *GenericReducer {
	return NewGenericReducer("Disconnect", "Universal disconnect reducer", func(ctx *ReducerContext, args []byte) ReducerResult {
		var timer *PerformanceTimer
		if b.performanceEnabled {
			timer = NewPerformanceTimer("Disconnect")
			defer timer.Stop()
		}

		identity := ctx.Sender
		LogInfo(fmt.Sprintf("Client disconnecting: %s", identity.Name))

		// Call custom disconnect handler if provided
		if b.onDisconnect != nil {
			if err := b.onDisconnect(ctx, identity); err != nil {
				LogWarn(fmt.Sprintf("Custom disconnect failed: %v", err))
			}
		}

		// Handle player session management if enabled
		if b.playerSessionEnabled {
			if err := b.handlePlayerDisconnection(ctx, identity); err != nil {
				LogWarn(fmt.Sprintf("Player disconnection cleanup failed: %v", err))
			}
		}

		LogInfo(fmt.Sprintf("Client disconnected: %s", identity.Name))
		return NewSuccessResult()
	})
}

// Helper methods for common patterns

// scheduleDefaultTimers schedules common game timers
func (b *LifecycleReducerBuilder) scheduleDefaultTimers(ctx *ReducerContext) error {
	// Schedule game tick timer
	if b.gameTickInterval > 0 {
		LogInfo(fmt.Sprintf("Scheduling game tick with interval: %v", b.gameTickInterval))
	}

	// Schedule cleanup timer
	if b.cleanupInterval > 0 {
		LogInfo(fmt.Sprintf("Scheduling cleanup with interval: %v", b.cleanupInterval))
	}

	return nil
}

// scheduleReducer is a helper to schedule a reducer (to be implemented with database context)
func (b *LifecycleReducerBuilder) scheduleReducer(ctx *ReducerContext, name string, args []byte, schedule interface{}) error {
	// TODO: Implement when database context has scheduler methods
	LogInfo(fmt.Sprintf("Scheduling reducer '%s'", name))
	return nil
}

// handlePlayerConnection handles generic player connection logic
func (b *LifecycleReducerBuilder) handlePlayerConnection(ctx *ReducerContext, identity *Identity) error {
	// This is a pattern that can be implemented by games
	// For now, just log the connection
	LogInfo(fmt.Sprintf("Player session starting for: %s", identity.Name))
	return nil
}

// handlePlayerDisconnection handles generic player disconnection logic
func (b *LifecycleReducerBuilder) handlePlayerDisconnection(ctx *ReducerContext, identity *Identity) error {
	// This is a pattern that can be implemented by games
	// For now, just log the disconnection
	LogInfo(fmt.Sprintf("Player session ending for: %s", identity.Name))
	return nil
}

// Common utility functions for lifecycle reducers

// PerformanceTimer provides timing utilities for reducer performance monitoring
type PerformanceTimer struct {
	Name      string
	StartTime time.Time
}

// NewPerformanceTimer creates a new performance timer
func NewPerformanceTimer(name string) *PerformanceTimer {
	return &PerformanceTimer{
		Name:      name,
		StartTime: time.Now(),
	}
}

// Stop stops the timer and logs the execution time
func (pt *PerformanceTimer) Stop() time.Duration {
	duration := time.Since(pt.StartTime)
	LogInfo(fmt.Sprintf("Performance[%s]: %v", pt.Name, duration))
	return duration
}

// Logging utilities specific to lifecycle reducers

// LogInfo logs an info message from a lifecycle reducer
func LogInfo(message string) {
	fmt.Printf("[INFO] %s\n", message)
}

// LogWarn logs a warning message from a lifecycle reducer
func LogWarn(message string) {
	fmt.Printf("[WARN] %s\n", message)
}

// LogError logs an error message from a lifecycle reducer
func LogError(message string) {
	fmt.Printf("[ERROR] %s\n", message)
}

// Configuration utilities for common game patterns

// GameConfig represents common game configuration that can be extended
type GameConfig interface {
	Validate() error
	GetWorldSize() uint32
	GetMaxPlayers() uint32
}

// TimerConfig represents timer configuration for games
type TimerConfig struct {
	GameTickInterval time.Duration `json:"game_tick_interval"`
	CleanupInterval  time.Duration `json:"cleanup_interval"`
	SpawnInterval    time.Duration `json:"spawn_interval"`
}

// DefaultTimerConfig returns default timer configuration
func DefaultTimerConfig() TimerConfig {
	return TimerConfig{
		GameTickInterval: 100 * time.Millisecond,
		CleanupInterval:  time.Minute,
		SpawnInterval:    time.Second,
	}
}

// Argument utilities for common reducer argument patterns

// ConnectArgs represents common connection arguments
type ConnectArgs struct {
	ClientVersion string                 `json:"client_version,omitempty"`
	Platform      string                 `json:"platform,omitempty"`
	Metadata      map[string]interface{} `json:"metadata,omitempty"`
}

// DisconnectArgs represents common disconnection arguments
type DisconnectArgs struct {
	Reason   string `json:"reason,omitempty"`
	Graceful bool   `json:"graceful"`
}

// ParseConnectArgs parses connection arguments from JSON
func ParseConnectArgs(data []byte) (*ConnectArgs, error) {
	var args ConnectArgs
	if len(data) == 0 {
		return &args, nil
	}

	if err := json.Unmarshal(data, &args); err != nil {
		return nil, fmt.Errorf("failed to parse connect args: %w", err)
	}

	return &args, nil
}

// ParseDisconnectArgs parses disconnection arguments from JSON
func ParseDisconnectArgs(data []byte) (*DisconnectArgs, error) {
	var args DisconnectArgs
	if len(data) == 0 {
		args.Graceful = true // Default to graceful disconnect
		return &args, nil
	}

	if err := json.Unmarshal(data, &args); err != nil {
		return nil, fmt.Errorf("failed to parse disconnect args: %w", err)
	}

	return &args, nil
}
