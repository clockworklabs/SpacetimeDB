// Package reducers - Tests for universal player action patterns
package reducers

import (
	"fmt"
	"testing"
	"time"
)

// TestPlayerActionBuilder tests the PlayerActionBuilder functionality
func TestPlayerActionBuilder(t *testing.T) {
	t.Run("BasicBuilderSetup", func(t *testing.T) {
		builder := NewPlayerActionBuilder()

		if !builder.performanceEnabled {
			t.Error("Performance monitoring should be enabled by default")
		}

		if !builder.validationEnabled {
			t.Error("Validation should be enabled by default")
		}

		if builder.cooldownEnabled {
			t.Error("Cooldowns should be disabled by default")
		}
	})

	t.Run("FluentConfiguration", func(t *testing.T) {
		builder := NewPlayerActionBuilder().
			WithPerformanceMonitoring(false).
			WithValidation(false).
			WithCooldowns(true).
			WithActionCooldown("test_action", 5*time.Second)

		if builder.performanceEnabled {
			t.Error("Performance monitoring should be disabled")
		}

		if builder.validationEnabled {
			t.Error("Validation should be disabled")
		}

		if !builder.cooldownEnabled {
			t.Error("Cooldowns should be enabled")
		}

		cooldown, exists := builder.actionCooldowns["test_action"]
		if !exists {
			t.Error("Test action cooldown should exist")
		}

		if cooldown != 5*time.Second {
			t.Errorf("Expected 5 second cooldown, got %v", cooldown)
		}
	})

	t.Run("HandlerAssignment", func(t *testing.T) {
		builder := NewPlayerActionBuilder()

		// Test handler assignment
		builder.OnPlayerLookup(func(ctx *ReducerContext, identity *Identity) (interface{}, error) {
			return "test_player", nil
		})

		builder.OnValidateAction(func(ctx *ReducerContext, player interface{}, actionName string, args interface{}) error {
			return nil
		})

		builder.OnExecuteAction(func(ctx *ReducerContext, player interface{}, args interface{}) error {
			return nil
		})

		if builder.onPlayerLookup == nil {
			t.Error("Player lookup handler should be assigned")
		}

		if builder.onValidateAction == nil {
			t.Error("Validate action handler should be assigned")
		}

		if builder.onExecuteAction == nil {
			t.Error("Execute action handler should be assigned")
		}
	})
}

// TestInputProcessor tests input processing functionality
func TestInputProcessor(t *testing.T) {
	t.Run("DefaultConfiguration", func(t *testing.T) {
		processor := NewInputProcessor()

		if processor.maxInputMagnitude != 1.0 {
			t.Errorf("Expected max magnitude 1.0, got %f", processor.maxInputMagnitude)
		}

		if processor.deadZone != 0.01 {
			t.Errorf("Expected dead zone 0.01, got %f", processor.deadZone)
		}
	})

	t.Run("FluentConfiguration", func(t *testing.T) {
		processor := NewInputProcessor().
			WithMaxMagnitude(2.0).
			WithDeadZone(0.1)

		if processor.maxInputMagnitude != 2.0 {
			t.Errorf("Expected max magnitude 2.0, got %f", processor.maxInputMagnitude)
		}

		if processor.deadZone != 0.1 {
			t.Errorf("Expected dead zone 0.1, got %f", processor.deadZone)
		}
	})

	t.Run("Vector2InputCalculations", func(t *testing.T) {
		// Test magnitude calculation
		vector := Vector2Input{X: 3.0, Y: 4.0}
		expectedMagnitude := 5.0 // 3-4-5 triangle

		magnitude := vector.Magnitude()
		tolerance := 0.000001
		if Abs(magnitude-expectedMagnitude) > tolerance {
			t.Errorf("Expected magnitude %f, got %f", expectedMagnitude, magnitude)
		}

		// Test normalization
		normalized := vector.Normalized()
		expectedX := 3.0 / 5.0
		expectedY := 4.0 / 5.0

		if Abs(normalized.X-expectedX) > tolerance || Abs(normalized.Y-expectedY) > tolerance {
			t.Errorf("Expected normalized (%f, %f), got (%f, %f)",
				expectedX, expectedY, normalized.X, normalized.Y)
		}

		// Test zero vector normalization
		zero := Vector2Input{X: 0, Y: 0}
		normalizedZero := zero.Normalized()
		if normalizedZero.X != 0 || normalizedZero.Y != 0 {
			t.Errorf("Expected zero vector to stay zero, got (%f, %f)",
				normalizedZero.X, normalizedZero.Y)
		}
	})

	t.Run("InputProcessing", func(t *testing.T) {
		processor := NewInputProcessor().
			WithMaxMagnitude(1.0).
			WithDeadZone(0.1)

		testCases := []struct {
			name     string
			input    Vector2Input
			expected Vector2Input
		}{
			{
				name:     "Normal input",
				input:    Vector2Input{X: 0.8, Y: 0.6},
				expected: Vector2Input{X: 0.8, Y: 0.6},
			},
			{
				name:     "Over-magnitude input",
				input:    Vector2Input{X: 2.0, Y: 1.5},
				expected: Vector2Input{X: 0.8, Y: 0.6}, // Normalized to max magnitude
			},
			{
				name:     "Below dead zone",
				input:    Vector2Input{X: 0.05, Y: 0.03},
				expected: Vector2Input{X: 0, Y: 0},
			},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				processed := processor.ProcessInput(tc.input)

				// Allow small floating point differences
				tolerance := 0.001
				if Abs(processed.X-tc.expected.X) > tolerance ||
					Abs(processed.Y-tc.expected.Y) > tolerance {
					t.Errorf("Expected (%f, %f), got (%f, %f)",
						tc.expected.X, tc.expected.Y, processed.X, processed.Y)
				}
			})
		}
	})

	t.Run("SpeedAndDirectionExtraction", func(t *testing.T) {
		processor := NewInputProcessor().WithMaxMagnitude(1.0)

		input := Vector2Input{X: 0.6, Y: 0.8} // Magnitude 1.0

		speed := processor.GetSpeed(input)
		direction := processor.GetDirection(input)

		expectedSpeed := float32(1.0)
		expectedDirection := Vector2Input{X: 0.6, Y: 0.8}

		if speed != expectedSpeed {
			t.Errorf("Expected speed %f, got %f", expectedSpeed, speed)
		}

		tolerance := 0.001
		if Abs(direction.X-expectedDirection.X) > tolerance ||
			Abs(direction.Y-expectedDirection.Y) > tolerance {
			t.Errorf("Expected direction (%f, %f), got (%f, %f)",
				expectedDirection.X, expectedDirection.Y, direction.X, direction.Y)
		}
	})
}

// TestActionValidator tests input validation functionality
func TestActionValidator(t *testing.T) {
	validator := NewActionValidator()

	t.Run("PlayerNameValidation", func(t *testing.T) {
		testCases := []struct {
			name        string
			playerName  string
			shouldError bool
		}{
			{"Valid name", "Player123", false},
			{"Valid short name", "A", false}, // Assuming min length is 1
			{"Empty name", "", true},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				err := validator.ValidatePlayerName(tc.playerName)

				if tc.shouldError && err == nil {
					t.Errorf("Expected error for name '%s', got none", tc.playerName)
				}

				if !tc.shouldError && err != nil {
					t.Errorf("Expected no error for name '%s', got %v", tc.playerName, err)
				}
			})
		}
	})

	t.Run("InputVectorValidation", func(t *testing.T) {
		testCases := []struct {
			name        string
			input       Vector2Input
			shouldError bool
		}{
			{"Valid input", Vector2Input{X: 0.5, Y: 0.8}, false},
			{"Valid negative", Vector2Input{X: -0.3, Y: 0.7}, false},
			{"Valid zero", Vector2Input{X: 0, Y: 0}, false},
			{"X out of range", Vector2Input{X: 15.0, Y: 2.0}, true},
			{"Y out of range", Vector2Input{X: 1.0, Y: 20.0}, true},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				err := validator.ValidateInputVector(tc.input)

				if tc.shouldError && err == nil {
					t.Errorf("Expected error for input (%f, %f), got none", tc.input.X, tc.input.Y)
				}

				if !tc.shouldError && err != nil {
					t.Errorf("Expected no error for input (%f, %f), got %v", tc.input.X, tc.input.Y, err)
				}
			})
		}
	})

	t.Run("ActionRequirementsValidation", func(t *testing.T) {
		playerState := map[string]interface{}{
			"level":      15,
			"health":     80,
			"has_weapon": true,
			"mana":       50,
		}

		testCases := []struct {
			name         string
			requirements map[string]interface{}
			shouldError  bool
		}{
			{
				"All requirements met",
				map[string]interface{}{
					"level":      10,
					"health":     70,
					"has_weapon": true,
				},
				false,
			},
			{
				"Level requirement not met",
				map[string]interface{}{
					"level": 20, // Player has 15, needs 20
				},
				true,
			},
			{
				"Missing property",
				map[string]interface{}{
					"missing_property": "value",
				},
				true,
			},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				err := validator.ValidateActionRequirements(playerState, tc.requirements)

				if tc.shouldError && err == nil {
					t.Errorf("Expected error for requirements %v, got none", tc.requirements)
				}

				if !tc.shouldError && err != nil {
					t.Errorf("Expected no error for requirements %v, got %v", tc.requirements, err)
				}
			})
		}
	})
}

// TestMathUtilities tests the math helper functions
func TestMathUtilities(t *testing.T) {
	t.Run("SqrtFunction", func(t *testing.T) {
		testCases := []struct {
			input     float64
			expected  float64
			tolerance float64
		}{
			{0, 0, 0.001},
			{1, 1, 0.001},
			{4, 2, 0.001},
			{9, 3, 0.001},
			{25, 5, 0.001},
			{0.25, 0.5, 0.001},
		}

		for _, tc := range testCases {
			t.Run(fmt.Sprintf("sqrt(%f)", tc.input), func(t *testing.T) {
				result := Sqrt(tc.input)

				if Abs(result-tc.expected) > tc.tolerance {
					t.Errorf("Expected sqrt(%f) = %f, got %f", tc.input, tc.expected, result)
				}
			})
		}

		// Test negative input
		result := Sqrt(-5)
		if result != 0 {
			t.Errorf("Expected sqrt(-5) = 0, got %f", result)
		}
	})

	t.Run("MinMaxFunctions", func(t *testing.T) {
		if Min(3.0, 5.0) != 3.0 {
			t.Error("Min(3.0, 5.0) should return 3.0")
		}

		if Min(5.0, 3.0) != 3.0 {
			t.Error("Min(5.0, 3.0) should return 3.0")
		}

		if Max(3.0, 5.0) != 5.0 {
			t.Error("Max(3.0, 5.0) should return 5.0")
		}

		if Max(5.0, 3.0) != 5.0 {
			t.Error("Max(5.0, 3.0) should return 5.0")
		}
	})

	t.Run("AbsFunction", func(t *testing.T) {
		if Abs(5.0) != 5.0 {
			t.Error("Abs(5.0) should return 5.0")
		}

		if Abs(-5.0) != 5.0 {
			t.Error("Abs(-5.0) should return 5.0")
		}

		if Abs(0.0) != 0.0 {
			t.Error("Abs(0.0) should return 0.0")
		}
	})
}

// TestPlayerActionExamples tests that the examples run without error
func TestPlayerActionExamples(t *testing.T) {
	t.Run("ExamplePlayerActionSetup", func(t *testing.T) {
		// This should not panic
		ExamplePlayerActionSetup()
	})

	t.Run("ExampleInputProcessing", func(t *testing.T) {
		// This should not panic
		ExampleInputProcessing()
	})

	t.Run("ExampleActionValidation", func(t *testing.T) {
		// This should not panic
		ExampleActionValidation()
	})

	t.Run("ExampleBuildEnterGameReducer", func(t *testing.T) {
		reducer := ExampleBuildEnterGameReducer()

		if reducer == nil {
			t.Error("EnterGame reducer should not be nil")
		}

		if reducer.Name() != "EnterGame" {
			t.Errorf("Expected reducer name 'EnterGame', got '%s'", reducer.Name())
		}
	})

	t.Run("ExampleBuildUpdateInputReducer", func(t *testing.T) {
		reducer := ExampleBuildUpdateInputReducer()

		if reducer == nil {
			t.Error("UpdatePlayerInput reducer should not be nil")
		}

		if reducer.Name() != "UpdatePlayerInput" {
			t.Errorf("Expected reducer name 'UpdatePlayerInput', got '%s'", reducer.Name())
		}
	})

	t.Run("RunAllPlayerActionExamples", func(t *testing.T) {
		// This should not panic
		RunAllPlayerActionExamples()
	})
}

// TestEntityManager tests entity management functionality
func TestEntityManager(t *testing.T) {
	// Create a mock context for testing
	mockCtx := &ReducerContext{
		// Add mock fields as needed
	}

	t.Run("EntityManagerCreation", func(t *testing.T) {
		manager := NewEntityManager(mockCtx)

		if manager == nil {
			t.Error("EntityManager should not be nil")
		}

		if manager.ctx != mockCtx {
			t.Error("EntityManager should store the provided context")
		}
	})

	t.Run("EntitySpawnConfig", func(t *testing.T) {
		config := EntitySpawnConfig{
			EntityType:   "test_entity",
			Position:     Vector2Input{X: 100, Y: 200},
			RandomizePos: false,
			WorldBounds:  Vector2Input{X: 1000, Y: 1000},
			Properties: map[string]interface{}{
				"health": 100,
				"level":  1,
			},
		}

		if config.EntityType != "test_entity" {
			t.Error("EntityType should be set correctly")
		}

		if config.Position.X != 100 || config.Position.Y != 200 {
			t.Error("Position should be set correctly")
		}

		if config.RandomizePos {
			t.Error("RandomizePos should be false")
		}

		if health, exists := config.Properties["health"]; !exists || health != 100 {
			t.Error("Properties should contain health = 100")
		}
	})
}

// TestPlayerStateManager tests player state management functionality
func TestPlayerStateManager(t *testing.T) {
	// Create a mock context for testing
	mockCtx := &ReducerContext{
		// Add mock fields as needed
	}

	t.Run("PlayerStateManagerCreation", func(t *testing.T) {
		manager := NewPlayerStateManager(mockCtx)

		if manager == nil {
			t.Error("PlayerStateManager should not be nil")
		}

		if manager.ctx != mockCtx {
			t.Error("PlayerStateManager should store the provided context")
		}
	})

	// Note: UpdatePlayerProperty, GetPlayerProperty, and ValidatePlayerState
	// are currently mock implementations that just log. In a real implementation,
	// these would interact with the actual database/state system.
}

// BenchmarkInputProcessing benchmarks input processing performance
func BenchmarkInputProcessing(b *testing.B) {
	processor := NewInputProcessor().
		WithMaxMagnitude(1.0).
		WithDeadZone(0.01)

	input := Vector2Input{X: 0.7, Y: 0.8}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		processor.ProcessInput(input)
	}
}

// BenchmarkVectorCalculations benchmarks vector math operations
func BenchmarkVectorCalculations(b *testing.B) {
	vector := Vector2Input{X: 3.0, Y: 4.0}

	b.Run("Magnitude", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			vector.Magnitude()
		}
	})

	b.Run("Normalized", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			vector.Normalized()
		}
	})
}
