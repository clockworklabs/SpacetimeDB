// Package reducers - Examples of using universal player action patterns
package reducers

import (
	"fmt"
	"time"
)

// Example 1: Basic Player Action Setup
// This shows how to use the PlayerActionBuilder for common actions

func ExamplePlayerActionSetup() {
	// Create a player action builder with common settings
	builder := NewPlayerActionBuilder().
		WithPerformanceMonitoring(true).
		WithValidation(true).
		WithCooldowns(true).
		WithActionCooldown("attack", 1*time.Second).
		WithActionCooldown("special_ability", 10*time.Second)

	// Set up player lookup handler (game-specific)
	builder.OnPlayerLookup(func(ctx *ReducerContext, identity *Identity) (interface{}, error) {
		// This would be implemented by your game to find the player
		LogInfo(fmt.Sprintf("Looking up player: %s", identity.Name))
		return map[string]interface{}{
			"id":     123,
			"name":   identity.Name,
			"level":  10,
			"health": 100,
		}, nil
	})

	// Set up action validation handler (game-specific)
	builder.OnValidateAction(func(ctx *ReducerContext, player interface{}, actionName string, args interface{}) error {
		playerData := player.(map[string]interface{})

		switch actionName {
		case "attack":
			// Validate player has enough health to attack
			if playerData["health"].(int) < 10 {
				return fmt.Errorf("not enough health to attack")
			}
		case "special_ability":
			// Validate player level requirement
			if playerData["level"].(int) < 5 {
				return fmt.Errorf("level too low for special ability")
			}
		}

		return nil
	})

	// Set up action execution handler (game-specific)
	builder.OnExecuteAction(func(ctx *ReducerContext, player interface{}, args interface{}) error {
		playerData := player.(map[string]interface{})
		LogInfo(fmt.Sprintf("Player %s executing action", playerData["name"]))
		// Game-specific action logic would go here
		return nil
	})

	// Build the reducers
	attackReducer := builder.BuildPlayerActionReducer("attack", "Player attack action")
	specialReducer := builder.BuildPlayerActionReducer("special_ability", "Player special ability")

	_ = attackReducer
	_ = specialReducer

	LogInfo("Player action setup complete")
}

// Example 2: Input Processing for Movement
// This shows how to use InputProcessor for player movement

func ExampleInputProcessing() {
	// Create an input processor with game-specific settings
	processor := NewInputProcessor().
		WithMaxMagnitude(1.0). // Maximum input strength
		WithDeadZone(0.1)      // Ignore small inputs

	// Example raw inputs from a player
	rawInputs := []Vector2Input{
		{X: 0.8, Y: 0.6},   // Normal movement
		{X: 2.0, Y: 1.5},   // Over-magnitude input
		{X: 0.05, Y: 0.03}, // Below dead zone
		{X: -0.7, Y: 0.9},  // Negative direction
	}

	for i, input := range rawInputs {
		// Process the input
		processed := processor.ProcessInput(input)
		speed := processor.GetSpeed(input)
		direction := processor.GetDirection(input)

		LogInfo(fmt.Sprintf("Input %d: Raw(%.2f,%.2f) -> Processed(%.2f,%.2f), Speed=%.2f",
			i+1, input.X, input.Y, processed.X, processed.Y, speed))
		LogInfo(fmt.Sprintf("  Direction: (%.2f,%.2f)", direction.X, direction.Y))
	}
}

// Example 3: Entity Management
// This shows how to use EntityManager for spawning and managing entities

func ExampleEntityManagement(ctx *ReducerContext) {
	entityManager := NewEntityManager(ctx)

	// Spawn a player entity
	playerConfig := EntitySpawnConfig{
		EntityType: "player",
		Position:   Vector2Input{X: 100, Y: 100},
		Properties: map[string]interface{}{
			"health": 100,
			"level":  1,
		},
	}

	player, err := entityManager.SpawnEntity(playerConfig)
	if err != nil {
		LogError(fmt.Sprintf("Failed to spawn player: %v", err))
		return
	}
	_ = player

	// Spawn random food entities
	for i := 0; i < 5; i++ {
		foodConfig := EntitySpawnConfig{
			EntityType:   "food",
			RandomizePos: true,
			WorldBounds:  Vector2Input{X: 1000, Y: 1000},
			Properties: map[string]interface{}{
				"nutrition": 10,
				"type":      "apple",
			},
		}

		food, err := entityManager.SpawnEntity(foodConfig)
		if err != nil {
			LogWarn(fmt.Sprintf("Failed to spawn food %d: %v", i, err))
			continue
		}
		_ = food
	}

	// Find entities by type
	allFood, err := entityManager.FindEntitiesByType("food")
	if err != nil {
		LogError(fmt.Sprintf("Failed to find food: %v", err))
		return
	}

	LogInfo(fmt.Sprintf("Found %d food entities", len(allFood)))
}

// Example 4: Player State Management
// This shows how to use PlayerStateManager for managing player properties

func ExamplePlayerStateManagement(ctx *ReducerContext) {
	stateManager := NewPlayerStateManager(ctx)
	playerID := uint32(123)

	// Update player properties
	stateManager.UpdatePlayerProperty(playerID, "health", 85)
	stateManager.UpdatePlayerProperty(playerID, "experience", 1500)
	stateManager.UpdatePlayerProperty(playerID, "last_action", time.Now())

	// Get player properties
	health, err := stateManager.GetPlayerProperty(playerID, "health")
	if err != nil {
		LogError(fmt.Sprintf("Failed to get health: %v", err))
		return
	}

	LogInfo(fmt.Sprintf("Player health: %v", health))

	// Validate player state for an action
	requirements := map[string]interface{}{
		"health":     85,   // Must have at least 85 health
		"level":      10,   // Must be level 10
		"has_weapon": true, // Must have a weapon
	}

	if err := stateManager.ValidatePlayerState(playerID, requirements); err != nil {
		LogWarn(fmt.Sprintf("Player state validation failed: %v", err))
	} else {
		LogInfo("Player meets all requirements")
	}
}

// Example 5: Action Validation
// This shows how to use ActionValidator for comprehensive validation

func ExampleActionValidation() {
	validator := NewActionValidator()

	// Test player name validation
	testNames := []string{
		"ValidName123", // Valid
		"A",            // Too short (if min length > 1)
		"ThisNameIsTooLongForTheGameSystemToHandle", // Too long
		"Name_With-Dashes",                          // Valid with allowed characters
	}

	for _, name := range testNames {
		if err := validator.ValidatePlayerName(name); err != nil {
			LogWarn(fmt.Sprintf("Invalid name '%s': %v", name, err))
		} else {
			LogInfo(fmt.Sprintf("Valid name: '%s'", name))
		}
	}

	// Test input vector validation
	testInputs := []Vector2Input{
		{X: 0.5, Y: 0.8},  // Valid
		{X: 15.0, Y: 2.0}, // Out of range
		{X: -0.3, Y: 0.7}, // Valid negative
		{X: 0, Y: 0},      // Valid zero
	}

	for i, input := range testInputs {
		if err := validator.ValidateInputVector(input); err != nil {
			LogWarn(fmt.Sprintf("Invalid input %d (%.1f,%.1f): %v", i+1, input.X, input.Y, err))
		} else {
			LogInfo(fmt.Sprintf("Valid input %d: (%.1f,%.1f)", i+1, input.X, input.Y))
		}
	}

	// Test action requirements validation
	playerState := map[string]interface{}{
		"level":      15,
		"health":     80,
		"has_weapon": true,
		"mana":       50,
	}

	requirements := map[string]interface{}{
		"level":      10,   // Player has 15, needs 10 ✓
		"health":     80,   // Player has 80, needs 80 ✓
		"has_weapon": true, // Player has true, needs true ✓
		"mana":       60,   // Player has 50, needs 60 ✗
	}

	if err := validator.ValidateActionRequirements(playerState, requirements); err != nil {
		LogWarn(fmt.Sprintf("Requirements not met: %v", err))
	} else {
		LogInfo("All action requirements met")
	}
}

// Example 6: Building a Complete Enter Game Reducer
// This shows how to combine universal patterns for a real game action

func ExampleBuildEnterGameReducer() *GenericReducer {
	builder := NewPlayerActionBuilder().
		WithPerformanceMonitoring(true).
		WithValidation(true)

	// Player lookup - find or create player record
	builder.OnPlayerLookup(func(ctx *ReducerContext, identity *Identity) (interface{}, error) {
		// This would be implemented by your game's database layer
		LogInfo(fmt.Sprintf("Looking up player for enter game: %s", identity.Name))

		// Return player data structure (game-specific)
		return map[string]interface{}{
			"identity": identity,
			"exists":   true, // Player already exists
		}, nil
	})

	// Validation - check if player can enter
	builder.OnValidateAction(func(ctx *ReducerContext, player interface{}, actionName string, args interface{}) error {
		// Parse enter game arguments
		enterArgs := args.([]byte)

		type EnterGameArgs struct {
			Name string `json:"name"`
		}

		var gameArgs EnterGameArgs
		if err := UnmarshalArgs(enterArgs, &gameArgs); err != nil {
			return fmt.Errorf("invalid enter game args: %v", err)
		}

		// Validate name using universal validator
		validator := NewActionValidator()
		if err := validator.ValidatePlayerName(gameArgs.Name); err != nil {
			return fmt.Errorf("invalid player name: %v", err)
		}

		LogInfo(fmt.Sprintf("Validated enter game for: %s", gameArgs.Name))
		return nil
	})

	// Execution - perform the enter game action
	builder.OnExecuteAction(func(ctx *ReducerContext, player interface{}, args interface{}) error {
		playerData := player.(map[string]interface{})
		identity := playerData["identity"].(*Identity)

		// Parse arguments again for execution
		enterArgs := args.([]byte)

		type EnterGameArgs struct {
			Name string `json:"name"`
		}

		var gameArgs EnterGameArgs
		UnmarshalArgs(enterArgs, &gameArgs)

		// Update player name (game-specific logic)
		LogInfo(fmt.Sprintf("Player %s entering game with name: %s", identity.Name, gameArgs.Name))

		// Spawn initial player entities (game-specific)
		entityManager := NewEntityManager(ctx)
		spawnConfig := EntitySpawnConfig{
			EntityType:   "player_avatar",
			RandomizePos: true,
			WorldBounds:  Vector2Input{X: 1000, Y: 1000},
			Properties: map[string]interface{}{
				"player_name": gameArgs.Name,
				"health":      100,
				"level":       1,
			},
		}

		_, err := entityManager.SpawnEntity(spawnConfig)
		if err != nil {
			return fmt.Errorf("failed to spawn player avatar: %v", err)
		}

		LogInfo(fmt.Sprintf("Successfully entered game: %s", gameArgs.Name))
		return nil
	})

	return builder.BuildPlayerActionReducer("EnterGame", "Player enters the game with a name")
}

// Example 7: Building an Update Input Reducer
// This shows how to use input processing for movement

func ExampleBuildUpdateInputReducer() *GenericReducer {
	builder := NewPlayerActionBuilder().
		WithPerformanceMonitoring(true).
		WithValidation(true)

	// Player lookup
	builder.OnPlayerLookup(func(ctx *ReducerContext, identity *Identity) (interface{}, error) {
		LogInfo(fmt.Sprintf("Looking up player for input update: %s", identity.Name))
		return map[string]interface{}{
			"identity":  identity,
			"player_id": 123,
		}, nil
	})

	// Validation
	builder.OnValidateAction(func(ctx *ReducerContext, player interface{}, actionName string, args interface{}) error {
		inputArgs := args.([]byte)

		type UpdateInputArgs struct {
			Direction Vector2Input `json:"direction"`
		}

		var updateArgs UpdateInputArgs
		if err := UnmarshalArgs(inputArgs, &updateArgs); err != nil {
			return fmt.Errorf("invalid input args: %v", err)
		}

		// Validate input vector
		validator := NewActionValidator()
		if err := validator.ValidateInputVector(updateArgs.Direction); err != nil {
			return fmt.Errorf("invalid input direction: %v", err)
		}

		return nil
	})

	// Execution
	builder.OnExecuteAction(func(ctx *ReducerContext, player interface{}, args interface{}) error {
		inputArgs := args.([]byte)

		type UpdateInputArgs struct {
			Direction Vector2Input `json:"direction"`
		}

		var updateArgs UpdateInputArgs
		UnmarshalArgs(inputArgs, &updateArgs)

		// Process input using universal processor
		processor := NewInputProcessor().
			WithMaxMagnitude(1.0).
			WithDeadZone(0.1)

		processedInput := processor.ProcessInput(updateArgs.Direction)
		speed := processor.GetSpeed(updateArgs.Direction)
		direction := processor.GetDirection(updateArgs.Direction)

		LogInfo(fmt.Sprintf("Processed input: original(%.2f,%.2f) -> processed(%.2f,%.2f), direction(%.2f,%.2f), speed=%.2f",
			updateArgs.Direction.X, updateArgs.Direction.Y, processedInput.X, processedInput.Y, direction.X, direction.Y, speed))

		// Update all player entities with new movement (game-specific)
		entityManager := NewEntityManager(ctx)
		playerData := player.(map[string]interface{})
		playerID := playerData["player_id"].(int)

		entities, err := entityManager.FindEntitiesByPlayer(uint32(playerID))
		if err != nil {
			return fmt.Errorf("failed to find player entities: %v", err)
		}

		LogInfo(fmt.Sprintf("Updated movement for %d entities", len(entities)))
		return nil
	})

	return builder.BuildPlayerActionReducer("UpdatePlayerInput", "Update player movement direction")
}

// RunAllPlayerActionExamples demonstrates all the player action patterns
func RunAllPlayerActionExamples() {
	LogInfo("=== Universal Player Action Patterns Examples ===")

	LogInfo("1. Basic Player Action Setup")
	ExamplePlayerActionSetup()

	LogInfo("2. Input Processing")
	ExampleInputProcessing()

	// The following examples would need a real ReducerContext
	// LogInfo("3. Entity Management")
	// ExampleEntityManagement(ctx)

	// LogInfo("4. Player State Management")
	// ExamplePlayerStateManagement(ctx)

	LogInfo("5. Action Validation")
	ExampleActionValidation()

	LogInfo("6. Complete Enter Game Reducer")
	enterGameReducer := ExampleBuildEnterGameReducer()
	LogInfo(fmt.Sprintf("Built reducer: %s", enterGameReducer.Name()))

	LogInfo("7. Update Input Reducer")
	inputReducer := ExampleBuildUpdateInputReducer()
	LogInfo(fmt.Sprintf("Built reducer: %s", inputReducer.Name()))

	LogInfo("=== All Player Action Examples Completed ===")
}
