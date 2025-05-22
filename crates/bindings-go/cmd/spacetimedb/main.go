package main

import (
	"context"
	"fmt"
	"os"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/pkg/spacetimedb"
)

func main() {
	// Initialize the SpacetimeDB client
	client, err := spacetimedb.NewClient(context.Background())
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to initialize SpacetimeDB client: %v\n", err)
		os.Exit(1)
	}
	defer client.Close()

	// TODO: Add command-line interface and main functionality
	fmt.Println("SpacetimeDB Go bindings initialized successfully")
}
