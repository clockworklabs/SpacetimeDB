package main

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"os/signal"
	"sort"
	"strings"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/cache"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ---------------------------------------------------------------------------
// Table types matching server schema
// ---------------------------------------------------------------------------

// User matches the server-side User table.
type User struct {
	Identity types.Identity
	Name     *string
	Online   bool
}

// Message matches the server-side Message table.
type Message struct {
	Sender types.Identity
	Sent   types.Timestamp
	Text   string
}

// ---------------------------------------------------------------------------
// bsatn.Serializable helper for string reducer args
// ---------------------------------------------------------------------------

type bsatnString string

func (s bsatnString) WriteBsatn(w bsatn.Writer) {
	w.PutString(string(s))
}

// ---------------------------------------------------------------------------
// TableDef implementations (for client cache)
// ---------------------------------------------------------------------------

type userTableDef struct{}

func (d *userTableDef) TableName() string { return "user" }

func (d *userTableDef) DecodeRow(r bsatn.Reader) (any, error) {
	identity, err := types.ReadIdentity(r)
	if err != nil {
		return nil, err
	}
	name, err := bsatn.ReadOption(r, func(r bsatn.Reader) (string, error) {
		return r.GetString()
	})
	if err != nil {
		return nil, err
	}
	online, err := r.GetBool()
	if err != nil {
		return nil, err
	}
	return &User{Identity: identity, Name: name, Online: online}, nil
}

func (d *userTableDef) EncodeRow(row any) []byte {
	u := row.(*User)
	w := bsatn.NewWriter(64)
	u.Identity.WriteBsatn(w)
	if u.Name != nil {
		w.PutSumTag(0)
		w.PutString(*u.Name)
	} else {
		w.PutSumTag(1)
	}
	w.PutBool(u.Online)
	return w.Bytes()
}

// PrimaryKey implements cache.TableDefWithPK, enabling OnUpdate detection.
func (d *userTableDef) PrimaryKey(row any) any {
	return row.(*User).Identity.String()
}

type messageTableDef struct{}

func (d *messageTableDef) TableName() string { return "message" }

func (d *messageTableDef) DecodeRow(r bsatn.Reader) (any, error) {
	sender, err := types.ReadIdentity(r)
	if err != nil {
		return nil, err
	}
	sent, err := types.ReadTimestamp(r)
	if err != nil {
		return nil, err
	}
	text, err := r.GetString()
	if err != nil {
		return nil, err
	}
	return &Message{Sender: sender, Sent: sent, Text: text}, nil
}

func (d *messageTableDef) EncodeRow(row any) []byte {
	m := row.(*Message)
	w := bsatn.NewWriter(128)
	m.Sender.WriteBsatn(w)
	m.Sent.WriteBsatn(w)
	w.PutString(m.Text)
	return w.Bytes()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func userNameOrIdentity(u *User) string {
	if u.Name != nil {
		return *u.Name
	}
	s := u.Identity.String()
	if len(s) > 8 {
		return s[:8]
	}
	return s
}

// findUserByIdentity looks up a User in the cache by Identity.
func findUserByIdentity(tc cache.TableCache, id types.Identity) *User {
	var found *User
	tc.Iter(func(row any) bool {
		u := row.(*User)
		if u.Identity.String() == id.String() {
			found = u
			return false
		}
		return true
	})
	return found
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

func main() {
	host := envOr("SPACETIMEDB_HOST", "http://localhost:3000")
	dbName := envOr("SPACETIMEDB_DB_NAME", "quickstart-chat")

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
		OnDisconnect(func(conn client.DbConnection, err error) {
			if err != nil {
				fmt.Fprintf(os.Stderr, "Disconnected: %v\n", err)
				os.Exit(1)
			}
			fmt.Println("Disconnected.")
			os.Exit(0)
		}).
		Build(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to connect: %v\n", err)
		os.Exit(1)
	}

	// Register table definitions
	conn.RegisterTable(&userTableDef{})
	conn.RegisterTable(&messageTableDef{})

	// Register callbacks
	userCache := conn.Cache().GetTable("user")
	messageCache := conn.Cache().GetTable("message")

	// When a new user joins, print a notification.
	userCache.OnInsert(func(row any) {
		u := row.(*User)
		if u.Online {
			fmt.Printf("User %s connected.\n", userNameOrIdentity(u))
		}
	})

	// When a user's status changes, print a notification.
	userCache.OnUpdate(func(oldRow any, newRow any) {
		old := oldRow.(*User)
		new_ := newRow.(*User)
		if (old.Name == nil) != (new_.Name == nil) || (old.Name != nil && new_.Name != nil && *old.Name != *new_.Name) {
			fmt.Printf("User %s renamed to %s.\n", userNameOrIdentity(old), userNameOrIdentity(new_))
		}
		if old.Online && !new_.Online {
			fmt.Printf("User %s disconnected.\n", userNameOrIdentity(new_))
		}
		if !old.Online && new_.Online {
			fmt.Printf("User %s connected.\n", userNameOrIdentity(new_))
		}
	})

	// When a new message is received, print it.
	messageCache.OnInsert(func(row any) {
		m := row.(*Message)
		senderName := "unknown"
		if u := findUserByIdentity(userCache, m.Sender); u != nil {
			senderName = userNameOrIdentity(u)
		}
		fmt.Printf("%s: %s\n", senderName, m.Text)
	})

	// Subscribe to both tables
	conn.Subscribe("SELECT * FROM user", "SELECT * FROM message").
		OnApplied(func() {
			// Print past messages sorted by timestamp
			var messages []*Message
			messageCache.Iter(func(row any) bool {
				messages = append(messages, row.(*Message))
				return true
			})
			sort.Slice(messages, func(i, j int) bool {
				return messages[i].Sent.Microseconds() < messages[j].Sent.Microseconds()
			})
			for _, m := range messages {
				senderName := "unknown"
				if u := findUserByIdentity(userCache, m.Sender); u != nil {
					senderName = userNameOrIdentity(u)
				}
				fmt.Printf("%s: %s\n", senderName, m.Text)
			}
			fmt.Println("Fully connected and all subscriptions applied.")
			fmt.Println("Use /name to set your name, or type a message!")
		}).
		Build()

	// Start the connection event loop in a goroutine
	go func() {
		if err := conn.Run(ctx); err != nil {
			fmt.Fprintf(os.Stderr, "Connection error: %v\n", err)
		}
		cancel()
	}()

	// Handle user input from stdin
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		line := scanner.Text()
		if name, ok := strings.CutPrefix(line, "/name "); ok {
			if err := conn.CallReducer("set_name", bsatnString(name)); err != nil {
				fmt.Fprintf(os.Stderr, "Failed to set name: %v\n", err)
			}
		} else {
			if err := conn.CallReducer("send_message", bsatnString(line)); err != nil {
				fmt.Fprintf(os.Stderr, "Failed to send message: %v\n", err)
			}
		}
	}
}
