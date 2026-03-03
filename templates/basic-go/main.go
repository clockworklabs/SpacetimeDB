package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// Person matches the server-side Person table.
type Person struct {
	Name string
}

// personTableDef implements cache.TableDef for the Person table.
type personTableDef struct{}

func (d *personTableDef) TableName() string { return "person" }

func (d *personTableDef) DecodeRow(r bsatn.Reader) (any, error) {
	name, err := r.GetString()
	if err != nil {
		return nil, err
	}
	return &Person{Name: name}, nil
}

func (d *personTableDef) EncodeRow(row any) []byte {
	p := row.(*Person)
	w := bsatn.NewWriter(32)
	w.PutString(p.Name)
	return w.Bytes()
}

func main() {
	host := envOr("SPACETIMEDB_HOST", "http://localhost:3000")
	dbName := envOr("SPACETIMEDB_DB_NAME", "my-db")

	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt)
	defer cancel()

	conn, err := client.NewDbConnection().
		WithUri(host).
		WithDatabaseName(dbName).
		OnConnect(func(conn client.DbConnection, identity types.Identity, token string) {
			fmt.Println("Connected to SpacetimeDB")
		}).
		OnConnectError(func(err error) {
			fmt.Fprintf(os.Stderr, "Connection error: %v\n", err)
			os.Exit(1)
		}).
		Build(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to connect: %v\n", err)
		os.Exit(1)
	}

	conn.RegisterTable(&personTableDef{})

	// Register callback for new person inserts
	personCache := conn.Cache().GetTable("person")
	personCache.OnInsert(func(row any) {
		p := row.(*Person)
		fmt.Printf("New person: %s\n", p.Name)
	})

	// Subscribe to the person table
	conn.Subscribe("SELECT * FROM person").
		OnApplied(func() {
			fmt.Println("Subscribed to the person table")
		}).
		Build()

	// Block until interrupted
	if err := conn.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "Connection error: %v\n", err)
	}
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
