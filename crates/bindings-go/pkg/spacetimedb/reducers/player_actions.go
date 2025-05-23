// Package reducers - Universal player action patterns for SpacetimeDB games
package reducers

import (
	"fmt"
	"time"
)

// PlayerActionBuilder provides utilities for building common player action reducers
type PlayerActionBuilder struct {
	// Configuration
	performanceEnabled bool
	validationEnabled  bool
	cooldownEnabled    bool

	// Player action handlers
	onPlayerLookup   func(*ReducerContext, *Identity) (interface{}, error)
	onValidateAction func(*ReducerContext, interface{}, string, interface{}) error
	onExecuteAction  func(*ReducerContext, interface{}, interface{}) error

	// Cooldown configuration
	actionCooldowns map[string]time.Duration
}

// NewPlayerActionBuilder creates a new player action builder
func NewPlayerActionBuilder() *PlayerActionBuilder {
	return &PlayerActionBuilder{
		performanceEnabled: true,
		validationEnabled:  true,
		cooldownEnabled:    false,
		actionCooldowns:    make(map[string]time.Duration),
	}
}

// WithPerformanceMonitoring enables/disables performance monitoring
func (b *PlayerActionBuilder) WithPerformanceMonitoring(enabled bool) *PlayerActionBuilder {
	b.performanceEnabled = enabled
	return b
}

// WithValidation enables/disables argument validation
func (b *PlayerActionBuilder) WithValidation(enabled bool) *PlayerActionBuilder {
	b.validationEnabled = enabled
	return b
}

// WithCooldowns enables/disables action cooldowns
func (b *PlayerActionBuilder) WithCooldowns(enabled bool) *PlayerActionBuilder {
	b.cooldownEnabled = enabled
	return b
}

// WithActionCooldown sets a cooldown for a specific action
func (b *PlayerActionBuilder) WithActionCooldown(action string, cooldown time.Duration) *PlayerActionBuilder {
	b.actionCooldowns[action] = cooldown
	return b
}

// OnPlayerLookup sets the player lookup handler
func (b *PlayerActionBuilder) OnPlayerLookup(handler func(*ReducerContext, *Identity) (interface{}, error)) *PlayerActionBuilder {
	b.onPlayerLookup = handler
	return b
}

// OnValidateAction sets the action validation handler
func (b *PlayerActionBuilder) OnValidateAction(handler func(*ReducerContext, interface{}, string, interface{}) error) *PlayerActionBuilder {
	b.onValidateAction = handler
	return b
}

// OnExecuteAction sets the action execution handler
func (b *PlayerActionBuilder) OnExecuteAction(handler func(*ReducerContext, interface{}, interface{}) error) *PlayerActionBuilder {
	b.onExecuteAction = handler
	return b
}

// BuildPlayerActionReducer creates a universal player action reducer
func (b *PlayerActionBuilder) BuildPlayerActionReducer(actionName, description string) *GenericReducer {
	return NewGenericReducer(actionName, description, func(ctx *ReducerContext, args []byte) ReducerResult {
		var timer *PerformanceTimer
		if b.performanceEnabled {
			timer = NewPerformanceTimer(actionName)
			defer timer.Stop()
		}

		identity := ctx.Sender
		LogInfo(fmt.Sprintf("Player %s executing action: %s", identity.Name, actionName))

		// Look up player
		var player interface{}
		var err error
		if b.onPlayerLookup != nil {
			player, err = b.onPlayerLookup(ctx, identity)
			if err != nil {
				return NewErrorResult(fmt.Errorf("Player lookup failed: %v", err))
			}
		}

		// Parse arguments
		var actionArgs interface{}
		if len(args) > 0 {
			// For now, pass raw args - games can implement their own parsing
			actionArgs = args
		}

		// Validate action
		if b.validationEnabled && b.onValidateAction != nil {
			if err := b.onValidateAction(ctx, player, actionName, actionArgs); err != nil {
				return NewErrorResult(fmt.Errorf("Action validation failed: %v", err))
			}
		}

		// Check cooldowns
		if b.cooldownEnabled {
			if err := b.checkActionCooldown(ctx, identity, actionName); err != nil {
				return NewErrorResult(fmt.Errorf("Action on cooldown: %v", err))
			}
		}

		// Execute action
		if b.onExecuteAction != nil {
			if err := b.onExecuteAction(ctx, player, actionArgs); err != nil {
				return NewErrorResult(fmt.Errorf("Action execution failed: %v", err))
			}
		}

		// Record cooldown
		if b.cooldownEnabled {
			b.recordActionCooldown(ctx, identity, actionName)
		}

		LogInfo(fmt.Sprintf("Player action %s completed successfully", actionName))
		return NewSuccessResult()
	})
}

// Helper methods

// checkActionCooldown checks if an action is on cooldown
func (b *PlayerActionBuilder) checkActionCooldown(ctx *ReducerContext, identity *Identity, actionName string) error {
	cooldown, exists := b.actionCooldowns[actionName]
	if !exists {
		return nil // No cooldown configured
	}

	// TODO: Implement cooldown tracking with database or memory store
	// For now, just log the cooldown check
	LogInfo(fmt.Sprintf("Checking cooldown for action %s (duration: %v)", actionName, cooldown))
	return nil
}

// recordActionCooldown records when an action was performed
func (b *PlayerActionBuilder) recordActionCooldown(ctx *ReducerContext, identity *Identity, actionName string) {
	// TODO: Implement cooldown recording with database or memory store
	LogInfo(fmt.Sprintf("Recording cooldown for action %s", actionName))
}

// Input processing utilities for common game patterns

// InputProcessor provides utilities for processing player input
type InputProcessor struct {
	maxInputMagnitude float64
	deadZone          float64
}

// NewInputProcessor creates a new input processor
func NewInputProcessor() *InputProcessor {
	return &InputProcessor{
		maxInputMagnitude: 1.0,
		deadZone:          0.01,
	}
}

// WithMaxMagnitude sets the maximum input magnitude
func (ip *InputProcessor) WithMaxMagnitude(max float64) *InputProcessor {
	ip.maxInputMagnitude = max
	return ip
}

// WithDeadZone sets the dead zone threshold
func (ip *InputProcessor) WithDeadZone(deadZone float64) *InputProcessor {
	ip.deadZone = deadZone
	return ip
}

// Vector2Input represents a 2D input vector
type Vector2Input struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}

// Magnitude returns the magnitude of the vector
func (v Vector2Input) Magnitude() float64 {
	return Sqrt(v.X*v.X + v.Y*v.Y)
}

// Normalized returns a normalized version of the vector
func (v Vector2Input) Normalized() Vector2Input {
	mag := v.Magnitude()
	if mag == 0 {
		return Vector2Input{X: 0, Y: 0}
	}
	return Vector2Input{X: v.X / mag, Y: v.Y / mag}
}

// ProcessInput processes raw input and applies normalization, clamping, and dead zone
func (ip *InputProcessor) ProcessInput(input Vector2Input) Vector2Input {
	// Apply dead zone
	if input.Magnitude() < ip.deadZone {
		return Vector2Input{X: 0, Y: 0}
	}

	// Normalize if magnitude exceeds maximum
	if input.Magnitude() > ip.maxInputMagnitude {
		normalized := input.Normalized()
		return Vector2Input{
			X: normalized.X * ip.maxInputMagnitude,
			Y: normalized.Y * ip.maxInputMagnitude,
		}
	}

	return input
}

// GetSpeed returns the speed component (0-1) from the input
func (ip *InputProcessor) GetSpeed(input Vector2Input) float32 {
	magnitude := input.Magnitude()
	if magnitude > ip.maxInputMagnitude {
		magnitude = ip.maxInputMagnitude
	}
	return float32(magnitude / ip.maxInputMagnitude)
}

// GetDirection returns the normalized direction from the input
func (ip *InputProcessor) GetDirection(input Vector2Input) Vector2Input {
	return input.Normalized()
}

// Entity management utilities for common game patterns

// EntityManager provides utilities for managing game entities
type EntityManager struct {
	ctx *ReducerContext
}

// NewEntityManager creates a new entity manager
func NewEntityManager(ctx *ReducerContext) *EntityManager {
	return &EntityManager{ctx: ctx}
}

// EntitySpawnConfig represents configuration for spawning entities
type EntitySpawnConfig struct {
	EntityType   string                 `json:"entity_type"`
	Position     Vector2Input           `json:"position,omitempty"`
	Properties   map[string]interface{} `json:"properties,omitempty"`
	RandomizePos bool                   `json:"randomize_pos"`
	WorldBounds  Vector2Input           `json:"world_bounds,omitempty"`
}

// SpawnEntity spawns an entity with the given configuration
func (em *EntityManager) SpawnEntity(config EntitySpawnConfig) (interface{}, error) {
	// TODO: Implement when entity system is available
	LogInfo(fmt.Sprintf("Spawning entity type: %s", config.EntityType))

	if config.RandomizePos {
		LogInfo("Randomizing entity position within world bounds")
	}

	return nil, nil
}

// DestroyEntity destroys an entity by ID
func (em *EntityManager) DestroyEntity(entityID uint32) error {
	// TODO: Implement when entity system is available
	LogInfo(fmt.Sprintf("Destroying entity: %d", entityID))
	return nil
}

// FindEntitiesByType finds all entities of a specific type
func (em *EntityManager) FindEntitiesByType(entityType string) ([]interface{}, error) {
	// TODO: Implement when entity system is available
	LogInfo(fmt.Sprintf("Finding entities of type: %s", entityType))
	return nil, nil
}

// FindEntitiesByPlayer finds all entities owned by a player
func (em *EntityManager) FindEntitiesByPlayer(playerID uint32) ([]interface{}, error) {
	// TODO: Implement when entity system is available
	LogInfo(fmt.Sprintf("Finding entities for player: %d", playerID))
	return nil, nil
}

// Player state management utilities

// PlayerStateManager provides utilities for managing player state
type PlayerStateManager struct {
	ctx *ReducerContext
}

// NewPlayerStateManager creates a new player state manager
func NewPlayerStateManager(ctx *ReducerContext) *PlayerStateManager {
	return &PlayerStateManager{ctx: ctx}
}

// UpdatePlayerProperty updates a specific property of a player
func (psm *PlayerStateManager) UpdatePlayerProperty(playerID uint32, property string, value interface{}) error {
	// TODO: Implement when player system is available
	LogInfo(fmt.Sprintf("Updating player %d property %s to: %v", playerID, property, value))
	return nil
}

// GetPlayerProperty gets a specific property of a player
func (psm *PlayerStateManager) GetPlayerProperty(playerID uint32, property string) (interface{}, error) {
	// TODO: Implement when player system is available
	LogInfo(fmt.Sprintf("Getting player %d property: %s", playerID, property))
	return nil, nil
}

// ValidatePlayerState validates the current state of a player
func (psm *PlayerStateManager) ValidatePlayerState(playerID uint32, requirements map[string]interface{}) error {
	// TODO: Implement when player system is available
	LogInfo(fmt.Sprintf("Validating state for player %d", playerID))

	for requirement, expectedValue := range requirements {
		LogInfo(fmt.Sprintf("Checking requirement %s = %v", requirement, expectedValue))
	}

	return nil
}

// Action validation utilities

// ActionValidator provides utilities for validating player actions
type ActionValidator struct {
	validator *ArgumentValidator
}

// NewActionValidator creates a new action validator
func NewActionValidator() *ActionValidator {
	return &ActionValidator{
		validator: NewArgumentValidator(),
	}
}

// ValidatePlayerName validates a player name
func (av *ActionValidator) ValidatePlayerName(name string) error {
	if err := av.validator.ValidateStringLength(name, 1, 50); err != nil {
		return err
	}

	// Additional validation - no special characters, etc.
	allowedPattern := "^[a-zA-Z0-9_-]+$"
	_ = allowedPattern // TODO: Implement regex validation

	return nil
}

// ValidateInputVector validates a 2D input vector
func (av *ActionValidator) ValidateInputVector(input Vector2Input) error {
	if err := av.validator.ValidateRange(input.X, -10.0, 10.0); err != nil {
		return fmt.Errorf("invalid X coordinate: %v", err)
	}

	if err := av.validator.ValidateRange(input.Y, -10.0, 10.0); err != nil {
		return fmt.Errorf("invalid Y coordinate: %v", err)
	}

	return nil
}

// ValidateActionRequirements validates that a player meets action requirements
func (av *ActionValidator) ValidateActionRequirements(playerState map[string]interface{}, requirements map[string]interface{}) error {
	for key, requiredValue := range requirements {
		playerValue, exists := playerState[key]
		if !exists {
			return fmt.Errorf("player missing required property: %s", key)
		}

		// Handle numeric comparisons (assume requirements are minimums)
		switch req := requiredValue.(type) {
		case int:
			if playerVal, ok := playerValue.(int); ok {
				if playerVal < req {
					return fmt.Errorf("player does not meet requirement %s: has %v, needs at least %v", key, playerValue, requiredValue)
				}
			} else {
				return fmt.Errorf("type mismatch for requirement %s: expected int", key)
			}
		case float64:
			if playerVal, ok := playerValue.(float64); ok {
				if playerVal < req {
					return fmt.Errorf("player does not meet requirement %s: has %v, needs at least %v", key, playerValue, requiredValue)
				}
			} else {
				return fmt.Errorf("type mismatch for requirement %s: expected float64", key)
			}
		default:
			// Simple equality check for non-numeric types
			if playerValue != requiredValue {
				return fmt.Errorf("player does not meet requirement %s: has %v, needs %v", key, playerValue, requiredValue)
			}
		}
	}

	return nil
}

// Helper math functions for game calculations

// Sqrt calculates square root (improved implementation)
func Sqrt(x float64) float64 {
	if x < 0 {
		return 0
	}

	if x == 0 || x == 1 {
		return x
	}

	// For values less than 1, we need a different approach
	if x < 1 {
		// Use the identity sqrt(x) = 1/sqrt(1/x) for x < 1
		return 1.0 / Sqrt(1.0/x)
	}

	// Newton's method for values >= 1
	result := x
	for i := 0; i < 10; i++ {
		newResult := (result + x/result) / 2
		if Abs(newResult-result) < 0.000001 {
			break
		}
		result = newResult
	}

	return result
}

// Min returns the minimum of two float64 values
func Min(a, b float64) float64 {
	if a < b {
		return a
	}
	return b
}

// Max returns the maximum of two float64 values
func Max(a, b float64) float64 {
	if a > b {
		return a
	}
	return b
}

// Abs returns the absolute value of a float64
func Abs(x float64) float64 {
	if x < 0 {
		return -x
	}
	return x
}
