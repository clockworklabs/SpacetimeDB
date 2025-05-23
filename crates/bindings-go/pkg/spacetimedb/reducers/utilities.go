// Package reducers - Universal utility functions for SpacetimeDB games
package reducers

import (
	"encoding/json"
	"fmt"
	"reflect"
	"time"
)

// Common utility functions that can be universally useful

// UnmarshalArgs unmarshals JSON arguments into a target struct
func UnmarshalArgs(data []byte, target interface{}) error {
	if len(data) == 0 {
		return nil
	}

	if err := json.Unmarshal(data, target); err != nil {
		return fmt.Errorf("failed to unmarshal arguments: %w", err)
	}

	return nil
}

// MarshalArgs marshals a struct into JSON arguments
func MarshalArgs(source interface{}) ([]byte, error) {
	if source == nil {
		return nil, nil
	}

	data, err := json.Marshal(source)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal arguments: %w", err)
	}

	return data, nil
}

// ValidateRequired checks if required fields are present in a struct
func ValidateRequired(target interface{}, requiredFields []string) error {
	val := reflect.ValueOf(target)
	if val.Kind() == reflect.Ptr {
		val = val.Elem()
	}

	if val.Kind() != reflect.Struct {
		return fmt.Errorf("target must be a struct")
	}

	typ := val.Type()

	for _, fieldName := range requiredFields {
		field, found := typ.FieldByName(fieldName)
		if !found {
			return fmt.Errorf("field %s not found", fieldName)
		}

		fieldVal := val.FieldByName(fieldName)
		if !fieldVal.IsValid() || isZero(fieldVal) {
			return fmt.Errorf("required field %s is missing or empty", fieldName)
		}

		// Special handling for string fields
		if field.Type.Kind() == reflect.String && fieldVal.String() == "" {
			return fmt.Errorf("required string field %s is empty", fieldName)
		}
	}

	return nil
}

// isZero checks if a reflect.Value is the zero value for its type
func isZero(v reflect.Value) bool {
	switch v.Kind() {
	case reflect.String:
		return v.String() == ""
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return v.Int() == 0
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return v.Uint() == 0
	case reflect.Float32, reflect.Float64:
		return v.Float() == 0
	case reflect.Bool:
		return !v.Bool()
	case reflect.Slice, reflect.Map, reflect.Array:
		return v.Len() == 0
	case reflect.Ptr, reflect.Interface:
		return v.IsNil()
	default:
		return false
	}
}

// Timer utilities for common game patterns

// TimerScheduler provides common timer scheduling patterns
type TimerScheduler struct {
	defaultInterval time.Duration
	ctx             *ReducerContext
}

// NewTimerScheduler creates a new timer scheduler
func NewTimerScheduler(ctx *ReducerContext, defaultInterval time.Duration) *TimerScheduler {
	return &TimerScheduler{
		defaultInterval: defaultInterval,
		ctx:             ctx,
	}
}

// ScheduleOnce schedules a reducer to run once after a delay
func (ts *TimerScheduler) ScheduleOnce(reducerName string, args []byte, delay time.Duration) error {
	// TODO: Implement when database context supports scheduling
	LogInfo(fmt.Sprintf("Scheduling %s to run once after %v", reducerName, delay))
	return nil
}

// ScheduleRepeating schedules a reducer to run repeatedly at intervals
func (ts *TimerScheduler) ScheduleRepeating(reducerName string, args []byte, interval time.Duration) error {
	// TODO: Implement when database context supports scheduling
	LogInfo(fmt.Sprintf("Scheduling %s to run every %v", reducerName, interval))
	return nil
}

// ScheduleAt schedules a reducer to run at a specific time
func (ts *TimerScheduler) ScheduleAt(reducerName string, args []byte, targetTime time.Time) error {
	// TODO: Implement when database context supports scheduling
	delay := time.Until(targetTime)
	LogInfo(fmt.Sprintf("Scheduling %s to run at %v (in %v)", reducerName, targetTime, delay))
	return nil
}

// Player session utilities for common game patterns

// PlayerSessionManager provides common player session management patterns
type PlayerSessionManager struct {
	ctx                       *ReducerContext
	sessionRestorationEnabled bool
	sessionTimeoutDuration    time.Duration
}

// NewPlayerSessionManager creates a new player session manager
func NewPlayerSessionManager(ctx *ReducerContext) *PlayerSessionManager {
	return &PlayerSessionManager{
		ctx:                       ctx,
		sessionRestorationEnabled: true,
		sessionTimeoutDuration:    30 * time.Minute,
	}
}

// WithSessionRestoration enables/disables session restoration
func (psm *PlayerSessionManager) WithSessionRestoration(enabled bool) *PlayerSessionManager {
	psm.sessionRestorationEnabled = enabled
	return psm
}

// WithSessionTimeout sets the session timeout duration
func (psm *PlayerSessionManager) WithSessionTimeout(duration time.Duration) *PlayerSessionManager {
	psm.sessionTimeoutDuration = duration
	return psm
}

// StartSession starts a new player session
func (psm *PlayerSessionManager) StartSession(identity *Identity) error {
	LogInfo(fmt.Sprintf("Starting session for player: %s", identity.Name))

	// TODO: Implement session logic with database operations
	// This would typically:
	// 1. Check for existing session
	// 2. Restore or create new session
	// 3. Update session state

	return nil
}

// EndSession ends a player session
func (psm *PlayerSessionManager) EndSession(identity *Identity, reason string) error {
	LogInfo(fmt.Sprintf("Ending session for player: %s (reason: %s)", identity.Name, reason))

	// TODO: Implement session cleanup with database operations
	// This would typically:
	// 1. Save session state
	// 2. Clean up temporary data
	// 3. Log session metrics

	return nil
}

// RestoreSession attempts to restore a previous session
func (psm *PlayerSessionManager) RestoreSession(identity *Identity) (bool, error) {
	if !psm.sessionRestorationEnabled {
		return false, nil
	}

	LogInfo(fmt.Sprintf("Attempting to restore session for player: %s", identity.Name))

	// TODO: Implement session restoration with database operations
	// This would typically:
	// 1. Look up previous session data
	// 2. Check if session is still valid (not expired)
	// 3. Restore session state

	return false, nil // No session to restore for now
}

// Database operation utilities

// DatabaseOperationHelper provides common database operation patterns
type DatabaseOperationHelper struct {
	ctx *ReducerContext
}

// NewDatabaseOperationHelper creates a new database operation helper
func NewDatabaseOperationHelper(ctx *ReducerContext) *DatabaseOperationHelper {
	return &DatabaseOperationHelper{ctx: ctx}
}

// WithTransaction executes a function within a database transaction context
func (doh *DatabaseOperationHelper) WithTransaction(fn func() error) error {
	// TODO: Implement transaction support when database context supports it
	LogInfo("Executing operation in transaction context")

	// For now, just execute the function
	err := fn()
	if err != nil {
		LogError(fmt.Sprintf("Transaction failed: %v", err))
		return err
	}

	LogInfo("Transaction completed successfully")
	return nil
}

// BatchOperation executes multiple operations as a batch
func (doh *DatabaseOperationHelper) BatchOperation(operations []func() error) error {
	LogInfo(fmt.Sprintf("Executing batch operation with %d operations", len(operations)))

	for i, op := range operations {
		if err := op(); err != nil {
			return fmt.Errorf("batch operation %d failed: %w", i, err)
		}
	}

	LogInfo("Batch operation completed successfully")
	return nil
}

// Retry executes an operation with retry logic
func (doh *DatabaseOperationHelper) Retry(operation func() error, maxRetries int, delay time.Duration) error {
	var lastErr error

	for attempt := 0; attempt <= maxRetries; attempt++ {
		if attempt > 0 {
			LogWarn(fmt.Sprintf("Retrying operation (attempt %d/%d)", attempt, maxRetries))
			time.Sleep(delay)
		}

		if err := operation(); err != nil {
			lastErr = err
			continue
		}

		return nil // Success
	}

	return fmt.Errorf("operation failed after %d retries: %w", maxRetries, lastErr)
}

// Configuration utilities for common game patterns

// ConfigurationManager provides common configuration management patterns
type ConfigurationManager struct {
	config map[string]interface{}
	ctx    *ReducerContext
}

// NewConfigurationManager creates a new configuration manager
func NewConfigurationManager(ctx *ReducerContext) *ConfigurationManager {
	return &ConfigurationManager{
		config: make(map[string]interface{}),
		ctx:    ctx,
	}
}

// Get retrieves a configuration value
func (cm *ConfigurationManager) Get(key string) (interface{}, bool) {
	value, exists := cm.config[key]
	return value, exists
}

// GetString retrieves a string configuration value
func (cm *ConfigurationManager) GetString(key string, defaultValue string) string {
	if value, exists := cm.config[key]; exists {
		if str, ok := value.(string); ok {
			return str
		}
	}
	return defaultValue
}

// GetInt retrieves an integer configuration value
func (cm *ConfigurationManager) GetInt(key string, defaultValue int) int {
	if value, exists := cm.config[key]; exists {
		if num, ok := value.(int); ok {
			return num
		}
		if num, ok := value.(float64); ok {
			return int(num)
		}
	}
	return defaultValue
}

// GetFloat retrieves a float configuration value
func (cm *ConfigurationManager) GetFloat(key string, defaultValue float64) float64 {
	if value, exists := cm.config[key]; exists {
		if num, ok := value.(float64); ok {
			return num
		}
		if num, ok := value.(int); ok {
			return float64(num)
		}
	}
	return defaultValue
}

// GetBool retrieves a boolean configuration value
func (cm *ConfigurationManager) GetBool(key string, defaultValue bool) bool {
	if value, exists := cm.config[key]; exists {
		if b, ok := value.(bool); ok {
			return b
		}
	}
	return defaultValue
}

// GetDuration retrieves a duration configuration value
func (cm *ConfigurationManager) GetDuration(key string, defaultValue time.Duration) time.Duration {
	if value, exists := cm.config[key]; exists {
		if str, ok := value.(string); ok {
			if duration, err := time.ParseDuration(str); err == nil {
				return duration
			}
		}
		if num, ok := value.(float64); ok {
			return time.Duration(num) * time.Millisecond
		}
	}
	return defaultValue
}

// Set sets a configuration value
func (cm *ConfigurationManager) Set(key string, value interface{}) {
	cm.config[key] = value
}

// LoadFromJSON loads configuration from JSON data
func (cm *ConfigurationManager) LoadFromJSON(data []byte) error {
	var newConfig map[string]interface{}
	if err := json.Unmarshal(data, &newConfig); err != nil {
		return fmt.Errorf("failed to parse configuration JSON: %w", err)
	}

	// Merge with existing config
	for key, value := range newConfig {
		cm.config[key] = value
	}

	return nil
}

// SaveToJSON saves configuration to JSON data
func (cm *ConfigurationManager) SaveToJSON() ([]byte, error) {
	data, err := json.Marshal(cm.config)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal configuration to JSON: %w", err)
	}
	return data, nil
}

// Validation utilities for common argument patterns

// ArgumentValidator provides common argument validation patterns
type ArgumentValidator struct{}

// NewArgumentValidator creates a new argument validator
func NewArgumentValidator() *ArgumentValidator {
	return &ArgumentValidator{}
}

// ValidateStringLength validates that a string is within specified length bounds
func (av *ArgumentValidator) ValidateStringLength(value string, minLength, maxLength int) error {
	if len(value) < minLength {
		return fmt.Errorf("string too short: %d < %d", len(value), minLength)
	}
	if len(value) > maxLength {
		return fmt.Errorf("string too long: %d > %d", len(value), maxLength)
	}
	return nil
}

// ValidateRange validates that a number is within specified bounds
func (av *ArgumentValidator) ValidateRange(value, min, max float64) error {
	if value < min {
		return fmt.Errorf("value too small: %f < %f", value, min)
	}
	if value > max {
		return fmt.Errorf("value too large: %f > %f", value, max)
	}
	return nil
}

// ValidateEnum validates that a value is one of the allowed enum values
func (av *ArgumentValidator) ValidateEnum(value string, allowedValues []string) error {
	for _, allowed := range allowedValues {
		if value == allowed {
			return nil
		}
	}
	return fmt.Errorf("invalid enum value: %s (allowed: %v)", value, allowedValues)
}

// Math utilities for common game calculations

// Clamp clamps a value between min and max bounds
func Clamp(value, min, max float32) float32 {
	if value < min {
		return min
	}
	if value > max {
		return max
	}
	return value
}

// ClampInt clamps an integer value between min and max bounds
func ClampInt(value, min, max int) int {
	if value < min {
		return min
	}
	if value > max {
		return max
	}
	return value
}

// Lerp performs linear interpolation between two values
func Lerp(a, b, t float32) float32 {
	return a + t*(b-a)
}

// MapRange maps a value from one range to another
func MapRange(value, fromMin, fromMax, toMin, toMax float32) float32 {
	return toMin + (value-fromMin)*(toMax-toMin)/(fromMax-fromMin)
}
