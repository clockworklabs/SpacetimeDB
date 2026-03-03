//go:build !wasip1

package runtime

// Stubs so the package compiles under standard Go for testing.
// The actual WASM exports are in exports.go.

// NewProcedureContext stub for non-WASM builds.
// The real implementation is in procedure_ctx.go which uses sys imports.
// This stub allows the package to compile for testing.
