package sdk

import (
	"os"
	"testing"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb"
	"github.com/stretchr/testify/require"
)

// compileSDKTestModule compiles the sdk-test module to WASM
func compileSDKTestModule(t *testing.T) string {
	// TODO: Implement WASM compilation
	// This should:
	// 1. Locate the sdk-test module source
	// 2. Compile it to WASM
	// 3. Return the path to the compiled WASM file
	return ""
}

// generateTestData creates test data for various types
func generateTestData(t *testing.T) map[string]interface{} {
	// TODO: Implement test data generation
	// This should create test data for:
	// - BTreeU32 table operations
	// - EveryPrimitiveStruct
	// - IndexedSimpleEnum
	return make(map[string]interface{})
}

// assertTableContents verifies the contents of a table
func assertTableContents(t *testing.T, tableID spacetimedb.TableID, expected []interface{}) {
	// TODO: Implement table content verification
	// This should:
	// 1. Scan the table
	// 2. Compare contents with expected values
	// 3. Handle BSATN encoding/decoding
}

// cleanupTestFiles removes temporary test files
func cleanupTestFiles(t *testing.T, files ...string) {
	for _, file := range files {
		if file != "" {
			err := os.Remove(file)
			require.NoError(t, err, "Failed to remove test file: %s", file)
		}
	}
}

// getSDKTestModulePath returns the path to the sdk-test module
func getSDKTestModulePath(t *testing.T) string {
	// TODO: Implement module path resolution
	// This should:
	// 1. Find the sdk-test module in the workspace
	// 2. Return its absolute path
	return ""
}
