package bsatn

import (
	"fmt"
	"log"
	"reflect"
	"regexp"
	"strings"
	"sync"
)

// ConstraintKind defines the type of a constraint.
type ConstraintKind string

const (
	ConstraintKindMinMax    ConstraintKind = "minMax"    // For numbers: Min, Max (can be int64, uint64 or float64)
	ConstraintKindMinMaxLen ConstraintKind = "minMaxLen" // For strings, bytes, arrays, lists: MinLen, MaxLen (int)
	ConstraintKindPattern   ConstraintKind = "pattern"   // For strings: RegexPattern (string)
	// Future: ConstraintKindEnumValues ConstraintKind = "enumValues"
)

// Constraint defines a validation rule for a type or its fields.
type Constraint struct {
	Kind ConstraintKind
	Path string // Optional: dot-separated path to a field. If empty, applies to the whole type.
	// For array/list elements, path could be "MyList[]" or "MyList[].Field"

	// Configuration for specific kinds (only relevant fields will be set based on Kind)
	Min          interface{} // For ConstraintKindMinMax (e.g., int64, uint64, float64)
	Max          interface{} // For ConstraintKindMinMax (e.g., int64, uint64, float64)
	MinLen       int64       // For ConstraintKindMinMaxLen (using int64 for general purpose length)
	MaxLen       int64       // For ConstraintKindMinMaxLen
	RegexPattern string      // For ConstraintKindPattern

	// Message string // Optional: Custom error message if this constraint fails.
}

// RegisteredTypeInfo holds metadata for a Go type that is mapped to a SpacetimeDB SATS type.
type RegisteredTypeInfo struct {
	GoType      reflect.Type
	SATSName    string        // Canonical SATS name (e.g., "MyModule.MyTable", "primitive.u32")
	RefID       uint32        // Canonical BSATN u32 reference ID for this SATS type
	Schema      AlgebraicType // BSATN AlgebraicType description of the type's structure
	Constraints []Constraint  // Validation constraints for this type

	// Future enhancements:
	// CustomSerializer   IStructuralReadWrite // If the type provides custom BSATN handling
	// Parameters         []ParameterInfo      // For generic/parameterized types if applicable
}

var (
	registryMutex      sync.RWMutex
	registryByGoType   = make(map[reflect.Type]*RegisteredTypeInfo)
	registryBySATSName = make(map[string]*RegisteredTypeInfo)
	registryByRefID    = make(map[uint32]*RegisteredTypeInfo) // New registry for u32 Ref IDs
)

// RegisterType associates a Go type with its SATS metadata and constraints.
// It takes an instance of the Go type (e.g., MyStruct{}) to derive its reflect.Type.
// refID is the canonical BSATN u32 reference ID for this SATS type.
// If the Go type or SATS name is already registered with conflicting information, an error is returned.
func RegisterType(goTypeInstance interface{}, satsName string, refID uint32, schema AlgebraicType, constraints ...Constraint) error {
	rt := reflect.TypeOf(goTypeInstance)
	if rt == nil {
		return fmt.Errorf("bsatn: cannot register nil type instance")
	}
	if rt.Kind() == reflect.Ptr {
		rt = rt.Elem()
	}

	if schema.Kind.String() == "unknown_atKind" {
		return fmt.Errorf("bsatn: cannot register type %v (SATS: %s, RefID: %d) with invalid schema kind %d", rt, satsName, refID, schema.Kind)
	}

	registryMutex.Lock()
	defer registryMutex.Unlock()

	info := &RegisteredTypeInfo{
		GoType:      rt,
		SATSName:    satsName,
		RefID:       refID, // Store the RefID
		Schema:      schema,
		Constraints: constraints,
	}

	// Check for conflicts
	if existing, found := registryByGoType[rt]; found {
		if existing.SATSName != satsName ||
			existing.RefID != refID || // Check RefID conflict
			!reflect.DeepEqual(existing.Schema, schema) ||
			!reflect.DeepEqual(existing.Constraints, constraints) {
			return fmt.Errorf("bsatn: type %v already registered with different SATS metadata (Name, RefID, Schema, or Constraints) (existing SATS: %s, RefID: %d; new SATS: %s, RefID: %d)", rt, existing.SATSName, existing.RefID, satsName, refID)
		}
	}
	if existing, found := registryBySATSName[satsName]; found {
		if existing.GoType != rt ||
			existing.RefID != refID || // Check RefID conflict
			!reflect.DeepEqual(existing.Schema, schema) ||
			!reflect.DeepEqual(existing.Constraints, constraints) {
			return fmt.Errorf("bsatn: SATS name '%s' already registered with a different Go type or metadata (RefID, Schema, or Constraints) (existing GoType: %v, RefID: %d; new GoType: %v, RefID: %d)", satsName, existing.GoType, existing.RefID, rt, refID)
		}
	}
	if existing, found := registryByRefID[refID]; found {
		if existing.GoType != rt ||
			existing.SATSName != satsName || // Check SATSName conflict
			!reflect.DeepEqual(existing.Schema, schema) ||
			!reflect.DeepEqual(existing.Constraints, constraints) {
			return fmt.Errorf("bsatn: RefID %d already registered with a different Go type or metadata (SATSName, Schema, or Constraints) (existing GoType: %v, SATSName: %s; new GoType: %v, SATSName: %s)", refID, existing.GoType, existing.SATSName, rt, satsName)
		}
	}

	registryByGoType[rt] = info
	registryBySATSName[satsName] = info
	registryByRefID[refID] = info // Populate the new registry
	log.Printf("[BSATN Registry] Registered type %v (SATS: %s, RefID: %d) with %d constraints", rt, satsName, refID, len(constraints))
	return nil
}

// MustRegisterType is like RegisterType but panics if registration fails.
func MustRegisterType(goTypeInstance interface{}, satsName string, refID uint32, schema AlgebraicType, constraints ...Constraint) {
	if err := RegisterType(goTypeInstance, satsName, refID, schema, constraints...); err != nil {
		panic(err)
	}
}

// GetTypeInfoByGoType retrieves metadata for a registered Go type.
func GetTypeInfoByGoType(goTypeInstance interface{}) (*RegisteredTypeInfo, bool) {
	rt := reflect.TypeOf(goTypeInstance)
	if rt == nil {
		return nil, false
	}
	if rt.Kind() == reflect.Ptr {
		rt = rt.Elem()
	}

	registryMutex.RLock()
	defer registryMutex.RUnlock()
	info, found := registryByGoType[rt]
	return info, found
}

// GetTypeInfoBySATSName retrieves metadata for a registered SATS type name.
func GetTypeInfoBySATSName(satsName string) (*RegisteredTypeInfo, bool) {
	registryMutex.RLock()
	defer registryMutex.RUnlock()
	info, found := registryBySATSName[satsName]
	return info, found
}

// ClearRegistry (for testing purposes) clears all registered types.
func ClearRegistry() {
	registryMutex.Lock()
	defer registryMutex.Unlock()
	registryByGoType = make(map[reflect.Type]*RegisteredTypeInfo)
	registryBySATSName = make(map[string]*RegisteredTypeInfo)
	registryByRefID = make(map[uint32]*RegisteredTypeInfo) // Clear the new registry
	log.Println("[BSATN Registry] Cleared")
}

// ValidationError describes a single validation failure.
// It includes the path to the field, the kind of constraint violated,
// and a descriptive message.
type ValidationError struct {
	Path           string         // Dot-separated path to the field, or empty if type-level
	ConstraintKind ConstraintKind // The kind of constraint that failed
	Message        string         // Human-readable error message
	ViolatedValue  interface{}    // The value that violated the constraint
}

func (ve ValidationError) Error() string {
	if ve.Path != "" {
		return fmt.Sprintf("validation failed for field '%s': %s (value: %#v)", ve.Path, ve.Message, ve.ViolatedValue)
	}
	return fmt.Sprintf("validation failed for type: %s (value: %#v)", ve.Message, ve.ViolatedValue)
}

// getValueByPath traverses a struct instance based on a dot-separated path
// and returns the reflect.Value of the target field.
// Currently supports direct field access and simple pointer dereferencing.
// Does not yet support slice/array indexing or map keys in path.
func getValueByPath(instance interface{}, path string) (reflect.Value, error) {
	val := reflect.ValueOf(instance)
	if path == "" {
		return val, nil // Constraint applies to the instance itself
	}

	parts := strings.Split(path, ".")
	currentVal := val

	for _, part := range parts {
		// Dereference pointers automatically along the path
		for currentVal.Kind() == reflect.Ptr {
			if currentVal.IsNil() {
				return reflect.Value{}, fmt.Errorf("bsatn: cannot access field '%s' on nil pointer in path '%s'", part, path)
			}
			currentVal = currentVal.Elem()
		}

		if currentVal.Kind() != reflect.Struct {
			return reflect.Value{}, fmt.Errorf("bsatn: path part '%s' in '%s' is not a struct, but %s", part, path, currentVal.Kind())
		}

		field := currentVal.FieldByName(part)
		if !field.IsValid() {
			return reflect.Value{}, fmt.Errorf("bsatn: field '%s' not found in path '%s' for type %s", part, path, currentVal.Type())
		}
		currentVal = field
	}
	return currentVal, nil
}

// Validate checks if a given Go instance conforms to the schema and constraints
// defined in its RegisteredTypeInfo. It returns a slice of ValidationErrors.
// If the slice is empty, validation passed.
func Validate(instance interface{}, info *RegisteredTypeInfo) []ValidationError {
	var validationErrors []ValidationError
	if info == nil {
		validationErrors = append(validationErrors, ValidationError{Message: "RegisteredTypeInfo is nil for validation"})
		return validationErrors
	}

	log.Printf("[Validate] Validating SATS type %s (Go type %T) with %d constraints",
		info.SATSName, instance, len(info.Constraints))

	// TODO: Implement schema validation against info.Schema (structural validation)
	// This would involve recursively checking the instance against the AlgebraicType schema.

	valOfInstance := reflect.ValueOf(instance)
	// If instance is a pointer, get the element it points to for validation.
	if valOfInstance.Kind() == reflect.Ptr {
		if valOfInstance.IsNil() {
			validationErrors = append(validationErrors, ValidationError{Message: "Cannot validate nil instance"})
			return validationErrors // Cannot validate a nil pointer further
		}
		valOfInstance = valOfInstance.Elem()
	}

	for _, constraint := range info.Constraints {
		var fieldValue reflect.Value
		var err error

		if constraint.Path == "" {
			fieldValue = valOfInstance // Constraint applies to the whole instance
		} else {
			fieldValue, err = getValueByPath(instance, constraint.Path)
			if err != nil {
				validationErrors = append(validationErrors, ValidationError{
					Path:           constraint.Path,
					ConstraintKind: constraint.Kind,
					Message:        fmt.Sprintf("Error accessing field for constraint: %v", err),
				})
				continue // Skip this constraint if path is invalid
			}
		}

		// Dereference pointers for the actual field value before checking
		skipConstraint := false
		for fieldValue.Kind() == reflect.Ptr {
			if fieldValue.IsNil() {
				// Nil pointer encountered – decide whether this is a validation error or a no-op.
				switch constraint.Kind {
				case ConstraintKindMinMaxLen:
					if constraint.MinLen > 0 {
						validationErrors = append(validationErrors, ValidationError{
							Path:           constraint.Path,
							ConstraintKind: constraint.Kind,
							Message:        fmt.Sprintf("length required to be at least %d, but field is nil", constraint.MinLen),
							ViolatedValue:  nil,
						})
					}
				// For MinMaxLen with MinLen == 0, or other constraint kinds, treat nil as absent and skip.
				case ConstraintKindPattern, ConstraintKindMinMax:
					// No error; constraint skipped.
				}
				skipConstraint = true
				break // Exit pointer-deref loop
			}
			fieldValue = fieldValue.Elem()
		}
		if skipConstraint {
			continue // Move to next constraint
		}

		if !fieldValue.IsValid() { // Should have been caught by getValueByPath, but as a safeguard
			log.Printf("[Validate] Field '%s' is invalid after potential dereferencing.", constraint.Path)
			continue
		}

		fieldActualValue := fieldValue.Interface()

		switch constraint.Kind {
		case ConstraintKindMinMaxLen:
			var currentLen int64 = -1
			switch fieldValue.Kind() {
			case reflect.String:
				currentLen = int64(fieldValue.Len())
			case reflect.Slice, reflect.Array, reflect.Map:
				if fieldValue.IsNil() && constraint.MinLen > 0 { // nil slice/map fails MinLen > 0
					validationErrors = append(validationErrors, ValidationError{
						Path:           constraint.Path,
						ConstraintKind: constraint.Kind,
						Message:        fmt.Sprintf("length required to be at least %d, but field is nil", constraint.MinLen),
						ViolatedValue:  nil,
					})
					continue
				} else if !fieldValue.IsNil() {
					currentLen = int64(fieldValue.Len())
				} else { // nil and MinLen is 0 or not set (or MaxLen is what matters)
					currentLen = 0 // Treat nil as 0 length for MaxLen check if MinLen allows it
				}
			default:
				validationErrors = append(validationErrors, ValidationError{
					Path:           constraint.Path,
					ConstraintKind: constraint.Kind,
					Message:        fmt.Sprintf("MinMaxLen constraint applied to unsupported type %s", fieldValue.Kind()),
					ViolatedValue:  fieldActualValue,
				})
				continue
			}

			if currentLen != -1 { // if length was determined
				if constraint.MinLen != 0 && currentLen < constraint.MinLen {
					validationErrors = append(validationErrors, ValidationError{
						Path:           constraint.Path,
						ConstraintKind: constraint.Kind,
						Message:        fmt.Sprintf("length %d is less than minimum %d", currentLen, constraint.MinLen),
						ViolatedValue:  fieldActualValue,
					})
				}
				if constraint.MaxLen != 0 && currentLen > constraint.MaxLen {
					validationErrors = append(validationErrors, ValidationError{
						Path:           constraint.Path,
						ConstraintKind: constraint.Kind,
						Message:        fmt.Sprintf("length %d is greater than maximum %d", currentLen, constraint.MaxLen),
						ViolatedValue:  fieldActualValue,
					})
				}
			}

		case ConstraintKindPattern:
			if fieldValue.Kind() != reflect.String {
				validationErrors = append(validationErrors, ValidationError{
					Path:           constraint.Path,
					ConstraintKind: constraint.Kind,
					Message:        fmt.Sprintf("Pattern constraint applied to non-numeric type %s", fieldValue.Kind()),
					ViolatedValue:  fieldActualValue,
				})
				continue
			}
			strValue := fieldValue.String()
			matched, errRegexp := regexp.MatchString(constraint.RegexPattern, strValue)
			if errRegexp != nil {
				validationErrors = append(validationErrors, ValidationError{
					Path:           constraint.Path,
					ConstraintKind: constraint.Kind,
					Message:        fmt.Sprintf("Invalid regex pattern in constraint: %v", errRegexp),
				})
				continue
			}
			if !matched {
				validationErrors = append(validationErrors, ValidationError{
					Path:           constraint.Path,
					ConstraintKind: constraint.Kind,
					Message:        fmt.Sprintf("value does not match pattern /%s/", constraint.RegexPattern),
					ViolatedValue:  strValue,
				})
			}

		case ConstraintKindMinMax:
			var valFloat float64
			ok := true
			switch fieldValue.Kind() {
			case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
				valFloat = float64(fieldValue.Int())
			case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64, reflect.Uintptr:
				valFloat = float64(fieldValue.Uint())
			case reflect.Float32, reflect.Float64:
				valFloat = fieldValue.Float()
				// If the value is exactly 0 and the minimum is greater than 0, treat this field as "absent" and skip validation.
				if valFloat == 0 && constraint.Min != nil {
					minFloatTmp, _ := toFloat64(constraint.Min)
					if minFloatTmp > 0 {
						continue // skip Min/Max checks for this constraint
					}
				}
			default:
				ok = false
			}
			if !ok {
				validationErrors = append(validationErrors, ValidationError{
					Path:           constraint.Path,
					ConstraintKind: constraint.Kind,
					Message:        fmt.Sprintf("MinMax constraint applied to non-numeric type %s", fieldValue.Kind()),
					ViolatedValue:  fieldActualValue,
				})
				continue
			}

			if constraint.Min != nil {
				minFloat, err := toFloat64(constraint.Min)
				if err != nil {
					validationErrors = append(validationErrors, ValidationError{Path: constraint.Path, ConstraintKind: constraint.Kind, Message: fmt.Sprintf("Invalid Min value in constraint: %v", err)})
					continue
				}
				if valFloat < minFloat {
					validationErrors = append(validationErrors, ValidationError{
						Path:           constraint.Path,
						ConstraintKind: constraint.Kind,
						Message:        fmt.Sprintf("value %v is less than minimum %v", fieldActualValue, constraint.Min),
						ViolatedValue:  fieldActualValue,
					})
				}
			}

			if constraint.Max != nil {
				maxFloat, err := toFloat64(constraint.Max)
				if err != nil {
					validationErrors = append(validationErrors, ValidationError{Path: constraint.Path, ConstraintKind: constraint.Kind, Message: fmt.Sprintf("Invalid Max value in constraint: %v", err)})
					continue
				}
				if valFloat > maxFloat {
					validationErrors = append(validationErrors, ValidationError{
						Path:           constraint.Path,
						ConstraintKind: constraint.Kind,
						Message:        fmt.Sprintf("value %v is greater than maximum %v", fieldActualValue, constraint.Max),
						ViolatedValue:  fieldActualValue,
					})
				}
			}

		default:
			validationErrors = append(validationErrors, ValidationError{
				Path:           constraint.Path,
				ConstraintKind: constraint.Kind,
				Message:        fmt.Sprintf("Unknown constraint kind: %s", constraint.Kind),
			})
		}
	}

	// Perform schema validation
	validateStructAgainstSchema(valOfInstance, info.Schema, "", &validationErrors)

	return validationErrors
}

// isCompatible checks if a Go kind is compatible with an expected BSATN kind.
// This is a simplified check and might need to be expanded.
func isCompatible(goKind reflect.Kind, expectedBSATNKind atKind) bool {
	switch expectedBSATNKind {
	case atBool:
		return goKind == reflect.Bool
	case atU8:
		return goKind == reflect.Uint8 || goKind == reflect.Uint // Allow Go uint for u8
	case atI8:
		return goKind == reflect.Int8 || goKind == reflect.Int // Allow Go int for i8
	case atU16:
		return goKind == reflect.Uint16 || goKind == reflect.Uint // Allow Go uint for u16
	case atI16:
		return goKind == reflect.Int16 || goKind == reflect.Int // Allow Go int for i16
	case atU32:
		return goKind == reflect.Uint32 || goKind == reflect.Uint // Allow Go uint for u32
	case atI32:
		return goKind == reflect.Int32 || goKind == reflect.Int // Allow Go int for i32
	case atU64:
		return goKind == reflect.Uint64 || goKind == reflect.Uint // Allow Go uint for u64
	case atI64:
		return goKind == reflect.Int64 || goKind == reflect.Int // Allow Go int for i64
	case atF32:
		return goKind == reflect.Float32
	case atF64:
		return goKind == reflect.Float64
	case atString:
		return goKind == reflect.String
	case atBytes:
		// Simplified check: atBytes expects a slice. More detailed check in validateStructAgainstSchema.
		return goKind == reflect.Slice
	default:
		// For complex types like Product, Sum, Array, Option, Ref, Opaque,
		// compatibility is checked structurally by validateStructAgainstSchema.
		return true // Assume compatible for now, structural check will follow
	}
}

func validateStructAgainstSchema(currentGoValue reflect.Value, expectedSchema AlgebraicType, currentPath string, validationErrors *[]ValidationError) {
	// Dereference pointer if currentGoValue is one
	val := currentGoValue
	if val.Kind() == reflect.Ptr {
		if val.IsNil() {
			// If the schema expects an optional type, this might be fine.
			// For now, if it's a nil pointer for a non-optional schema part, it's a mismatch.
			// This needs refinement with OptionType handling.
			// For ProductType, a nil pointer to a struct is generally not valid unless the whole product is optional.
			if expectedSchema.Kind != atOption { // Basic check, will need actual OptionType handling
				*validationErrors = append(*validationErrors, ValidationError{
					Path:    currentPath,
					Message: fmt.Sprintf("Schema mismatch: expected a %s, got nil pointer", expectedSchema.Kind.String()),
				})
			}
			return // Cannot validate further if nil
		}
		val = val.Elem()
	}

	switch expectedSchema.Kind {
	case atProduct:
		if val.Kind() != reflect.Struct {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema mismatch: expected a struct for ProductType, got %s", val.Kind()),
			})
			return
		}
		if expectedSchema.Product == nil {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: "Schema error: ProductType has nil Product",
			})
			return
		}

		for _, fieldSchema := range expectedSchema.Product.Elements {
			if fieldSchema.Name == nil {
				*validationErrors = append(*validationErrors, ValidationError{
					Path:    currentPath,
					Message: "Schema error: ProductElement has nil Name",
				})
				continue
			}
			fieldName := *fieldSchema.Name
			fieldPath := fieldName
			if currentPath != "" {
				fieldPath = currentPath + "." + fieldName
			}

			goField := val.FieldByName(fieldName)
			if !goField.IsValid() {
				*validationErrors = append(*validationErrors, ValidationError{
					Path:    fieldPath,
					Message: fmt.Sprintf("Schema mismatch: struct %s is missing field '%s' defined in ProductType schema", val.Type().Name(), fieldName),
				})
				continue
			}
			validateStructAgainstSchema(goField, fieldSchema.Type, fieldPath, validationErrors)
		}

	case atArray:
		if val.Kind() != reflect.Slice && val.Kind() != reflect.Array {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema mismatch: expected a slice or array for ArrayType, got %s", val.Kind()),
			})
			return
		}
		if expectedSchema.Array == nil {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: "Schema error: ArrayType has nil Array definition",
			})
			return
		}
		// Ensure we are accessing the .Elem field of the ArrayType struct
		arrayElementSchema := expectedSchema.Array.Elem // This is the AlgebraicType of the array elements
		if arrayElementSchema.Kind == atUnknown {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: "Schema error: ArrayType Element has unknown kind",
			})
			return
		}
		for i := 0; i < val.Len(); i++ {
			elemPath := fmt.Sprintf("%s[%d]", currentPath, i)
			validateStructAgainstSchema(val.Index(i), arrayElementSchema, elemPath, validationErrors)
		}

	case atSum:
		if expectedSchema.Sum == nil || len(expectedSchema.Sum.Variants) == 0 {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: "Schema error: SumType has nil or empty Variants list",
			})
			return
		}

		// Get RegisteredTypeInfo for the current Go value to find its SATS name/schema.
		// val is already dereferenced.
		instanceInfo, foundInstanceInfo := GetTypeInfoByGoType(val.Interface())
		// Note: val.Interface() is needed because GetTypeInfoByGoType expects an interface{}.
		// If val is from an unexported field, val.Interface() might panic.
		// However, for schema validation, we typically deal with exported fields or top-level structs.

		if !foundInstanceInfo {
			// If the Go type isn't registered, we can't reliably match it to a sum variant by SATS name.
			// We could try to match based on structural compatibility if names are absent or don't match,
			// but that's much more complex. For now, require registration for sum variant matching.
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema validation error: Go type %s at path '%s' is not registered, cannot match to SumType variants.", val.Type().String(), currentPath),
			})
			return
		}

		matchedVariant := false
		for _, schemaVariant := range expectedSchema.Sum.Variants {
			if schemaVariant.Name == nil {
				log.Printf("[Validate Schema] SumType variant at path '%s' has nil Name, skipping.", currentPath)
				continue
			}

			// Try to match by SATS name first (most reliable if Go types are registered correctly)
			if instanceInfo.SATSName == *schemaVariant.Name {
				log.Printf("[Validate Schema] SumType variant matched by SATSName: '%s' for path '%s'. Validating against variant schema.", *schemaVariant.Name, currentPath)
				validateStructAgainstSchema(val, schemaVariant.Type, currentPath, validationErrors) // currentPath refers to the location of the sum type itself
				matchedVariant = true
				break
			}

			// Fallback: If SATSNames don't match, or if instanceInfo.SATSName is generic,
			// we could attempt a structural match between instanceInfo.Schema and schemaVariant.Type.
			// This is complex. For now, we rely on SATS name matching.
			// A very basic structural check could be: DeepEqual(instanceInfo.Schema, schemaVariant.Type)
			// if reflect.DeepEqual(instanceInfo.Schema, schemaVariant.Type) {
			// 	log.Printf("[Validate Schema] SumType variant matched by schema DeepEqual for path '%s'. Validating against variant schema.", currentPath)
			// 	validateStructAgainstSchema(val, schemaVariant.Type, currentPath, validationErrors)
			// 	matchedVariant = true
			// 	break
			// }
		}

		if !matchedVariant {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema mismatch: Go type %s (SATS: %s) at path '%s' does not match any defined variants of the SumType schema.", val.Type().String(), instanceInfo.SATSName, currentPath),
			})
		}

	case atOption:
		// For atOption, the Go type MUST be a pointer.
		if currentGoValue.Kind() != reflect.Ptr {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema mismatch: expected a pointer for OptionType, got %s", currentGoValue.Kind()),
			})
			return
		}
		// If the pointer is nil, it's a valid "None" case for the option.
		if currentGoValue.IsNil() {
			log.Printf("[Validate Schema] OptionType at path '%s' is nil (None), valid.", currentPath)
			return // Valid nil pointer for an option
		}

		// If non-nil, validate the pointed-to value against the inner type of the option.
		if expectedSchema.Option == nil {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: "Schema error: OptionType schema has no inner type defined (Option field is nil)",
			})
			return
		}
		// val was already dereferenced at the beginning of the function if currentGoValue was a Ptr.
		// So, we pass `val` (the dereferenced value) to the recursive call.
		validateStructAgainstSchema(val, *expectedSchema.Option, currentPath, validationErrors)

	case atRef:
		registryMutex.RLock() // Need to lock for reading global registry
		referencedInfo, found := registryByRefID[expectedSchema.Ref]
		registryMutex.RUnlock()

		if !found {
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema error: unknown RefID %d encountered at path '%s'. Type not registered with this RefID.", expectedSchema.Ref, currentPath),
			})
			return
		}

		// Check if the Go type of the current value is assignable to the registered Go type for the reference.
		// This handles cases like interfaces or if the exact type is expected.
		// val is already dereferenced if it was a pointer.
		actualGoType := val.Type()
		expectedGoType := referencedInfo.GoType

		if actualGoType != expectedGoType {
			// Allow assignability for interfaces or if one is a pointer to the other in some cases (though val is usually dereferenced)
			// A simple check for now. This might need refinement based on how Go types map to SATS refs (e.g. interface vs concrete type)
			if !actualGoType.AssignableTo(expectedGoType) {
				*validationErrors = append(*validationErrors, ValidationError{
					Path:    currentPath,
					Message: fmt.Sprintf("Schema mismatch: type %s for path '%s' is not assignable to expected Go type %s for RefID %d (SATS: %s)", actualGoType.String(), currentPath, expectedGoType.String(), expectedSchema.Ref, referencedInfo.SATSName),
				})
				// Don't return yet, still try to validate against the schema if types are different but might be structurally compatible in some loose sense
				// However, for strict schema validation, a type mismatch here is usually critical.
			}
		}
		// Recursively validate against the schema of the referenced type.
		// The path for the referenced type's fields will be relative to itself, so we use an empty string for the new base path.
		// Or, if we want to keep the full path, we pass currentPath.
		// Let's pass currentPath to maintain full context.
		validateStructAgainstSchema(val, referencedInfo.Schema, currentPath, validationErrors)

	case atOpaque:
		log.Printf("[Validate Schema] OpaqueType encountered at path: %s. Assuming valid.", currentPath)

	// Primitive types
	case atBool, atU8, atI8, atU16, atI16, atU32, atI32, atU64, atI64, atF32, atF64, atString, atBytes:
		// Dereference if the Go value is a pointer to a primitive type
		// The main dereference at the start of the function handles currentGoValue being a pointer.
		// This handles cases where a struct field is a pointer to a primitive.
		finalVal := val
		if (expectedSchema.Kind != atOption) && finalVal.Kind() == reflect.Ptr { // Don't deref if schema is Option (already handled)
			if finalVal.IsNil() {
				// A nil pointer to a primitive for a non-optional schema type is a mismatch.
				*validationErrors = append(*validationErrors, ValidationError{
					Path:    currentPath,
					Message: fmt.Sprintf("Schema mismatch: expected non-nil value for %s at path '%s', got nil pointer", expectedSchema.Kind.String(), currentPath),
				})
				return
			}
			finalVal = finalVal.Elem()
		}

		if !isCompatible(finalVal.Kind(), expectedSchema.Kind) {
			expectedGoKindStr := "unknown" // Placeholder
			// Attempt to get a more descriptive expected Go kind string
			switch expectedSchema.Kind {
			case atBool:
				expectedGoKindStr = "bool"
			case atU8:
				expectedGoKindStr = "uint8 or uint"
			case atI8:
				expectedGoKindStr = "int8 or int"
			case atU16:
				expectedGoKindStr = "uint16 or uint"
			case atI16:
				expectedGoKindStr = "int16 or int"
			case atU32:
				expectedGoKindStr = "uint32 or uint"
			case atI32:
				expectedGoKindStr = "int32 or int"
			case atU64:
				expectedGoKindStr = "uint64 or uint"
			case atI64:
				expectedGoKindStr = "int64 or int"
			case atF32:
				expectedGoKindStr = "float32"
			case atF64:
				expectedGoKindStr = "float64"
			case atString:
				expectedGoKindStr = "string"
			case atBytes:
				expectedGoKindStr = "[]byte"
			}
			*validationErrors = append(*validationErrors, ValidationError{
				Path:    currentPath,
				Message: fmt.Sprintf("Schema mismatch: expected type compatible with %s (e.g., %s), got %s", expectedSchema.Kind.String(), expectedGoKindStr, finalVal.Kind().String()),
			})
		}
		// Specific check for atBytes
		if expectedSchema.Kind == atBytes && finalVal.Kind() == reflect.Slice {
			if finalVal.Type().Elem().Kind() != reflect.Uint8 {
				*validationErrors = append(*validationErrors, ValidationError{
					Path:    currentPath,
					Message: fmt.Sprintf("Schema mismatch: expected []byte for atBytes, got slice of %s", finalVal.Type().Elem().Kind().String()),
				})
			}
		}

	case atUnknown:
		*validationErrors = append(*validationErrors, ValidationError{
			Path:    currentPath,
			Message: fmt.Sprintf("Schema error: encountered an unknown AlgebraicType kind at path '%s'", currentPath),
		})
	default:
		*validationErrors = append(*validationErrors, ValidationError{
			Path:    currentPath,
			Message: fmt.Sprintf("Internal schema validation error: unhandled AlgebraicType.Kind %s at path '%s'", expectedSchema.Kind.String(), currentPath),
		})
	}
}

// toFloat64 is a helper to convert various numeric types to float64 for comparison.
func toFloat64(val interface{}) (float64, error) {
	switch v := val.(type) {
	case int:
		return float64(v), nil
	case int8:
		return float64(v), nil
	case int16:
		return float64(v), nil
	case int32:
		return float64(v), nil
	case int64:
		return float64(v), nil
	case uint:
		return float64(v), nil
	case uint8:
		return float64(v), nil
	case uint16:
		return float64(v), nil
	case uint32:
		return float64(v), nil
	case uint64:
		return float64(v), nil // Potential precision loss for very large uint64 > 2^53
	case float32:
		return float64(v), nil
	case float64:
		return v, nil
	default:
		return 0, fmt.Errorf("unsupported numeric type for Min/Max constraint: %T", val)
	}
}
