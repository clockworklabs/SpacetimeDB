package client

import (
	"context"
	"sync/atomic"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/cache"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/protocol"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/ws"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// DbConnectionBuilder builds a DbConnection.
type DbConnectionBuilder interface {
	WithUri(uri string) DbConnectionBuilder
	WithDatabaseName(nameOrAddress string) DbConnectionBuilder
	WithToken(token string) DbConnectionBuilder
	WithCompression(c protocol.Compression) DbConnectionBuilder
	OnConnect(fn func(conn DbConnection, identity types.Identity, token string)) DbConnectionBuilder
	OnConnectError(fn func(err error)) DbConnectionBuilder
	OnDisconnect(fn func(conn DbConnection, err error)) DbConnectionBuilder
	Build(ctx context.Context) (DbConnection, error)
}

// DbConnection is the primary interface for interacting with a SpacetimeDB database.
type DbConnection interface {
	Identity() types.Identity
	ConnectionId() types.ConnectionId
	Token() string
	IsActive() bool
	Subscribe(queries ...string) SubscriptionBuilder
	CallReducer(reducer string, args bsatn.Serializable) error
	OneOffQuery(query string) ([][]byte, error)
	Disconnect() error
	Run(ctx context.Context) error
	RegisterTable(def cache.TableDef)
	Cache() cache.ClientCache
}

// NewDbConnection returns a new DbConnectionBuilder.
func NewDbConnection() DbConnectionBuilder {
	return &dbConnectionBuilder{
		compression: protocol.CompressionNone,
	}
}

type dbConnectionBuilder struct {
	uri            string
	database       string
	token          string
	compression    protocol.Compression
	onConnect      func(DbConnection, types.Identity, string)
	onConnectError func(error)
	onDisconnect   func(DbConnection, error)
}

func (b *dbConnectionBuilder) WithUri(uri string) DbConnectionBuilder {
	b.uri = uri
	return b
}

func (b *dbConnectionBuilder) WithDatabaseName(name string) DbConnectionBuilder {
	b.database = name
	return b
}

func (b *dbConnectionBuilder) WithToken(token string) DbConnectionBuilder {
	b.token = token
	return b
}

func (b *dbConnectionBuilder) WithCompression(c protocol.Compression) DbConnectionBuilder {
	b.compression = c
	return b
}

func (b *dbConnectionBuilder) OnConnect(fn func(DbConnection, types.Identity, string)) DbConnectionBuilder {
	b.onConnect = fn
	return b
}

func (b *dbConnectionBuilder) OnConnectError(fn func(error)) DbConnectionBuilder {
	b.onConnectError = fn
	return b
}

func (b *dbConnectionBuilder) OnDisconnect(fn func(DbConnection, error)) DbConnectionBuilder {
	b.onDisconnect = fn
	return b
}

func (b *dbConnectionBuilder) Build(ctx context.Context) (DbConnection, error) {
	wsConn, err := ws.NewConnection().
		WithUri(b.uri).
		WithDatabaseName(b.database).
		WithToken(b.token).
		Build(ctx)
	if err != nil {
		if b.onConnectError != nil {
			b.onConnectError(err)
		}
		return nil, &ConnectionError{Message: "failed to connect", Err: err}
	}

	conn := &dbConnection{
		ws:             wsConn,
		cache:          cache.NewClientCache(),
		commands:       make(chan *command, 256),
		incoming:       make(chan []byte, 64),
		compression:    b.compression,
		onConnect:      b.onConnect,
		onConnectError: b.onConnectError,
		onDisconnect:   b.onDisconnect,
	}

	return conn, nil
}

// command types for the channel-based event loop.
type commandType int

const (
	cmdCallReducer commandType = iota
	cmdSubscribe
	cmdUnsubscribe
	cmdOneOffQuery
	cmdDisconnect
)

type command struct {
	typ        commandType
	reducer    string
	args       []byte
	queries    []string
	querySetID uint32
	onApplied  func()
	result     chan any
}

// oneOffResult is the internal result passed back through the one-off query result channel.
type oneOffResult struct {
	rows [][]byte
	err  error
}

type dbConnection struct {
	ws          ws.Connection
	cache       cache.ClientCache
	commands    chan *command
	incoming    chan []byte
	compression protocol.Compression

	identity     types.Identity
	connectionID types.ConnectionId
	token        string

	active         atomic.Bool
	nextRequestID  atomic.Uint32
	nextQuerySetID atomic.Uint32

	onConnect      func(DbConnection, types.Identity, string)
	onConnectError func(error)
	onDisconnect   func(DbConnection, error)

	// State maps only accessed from the Run() goroutine event loop.
	subscriptionCallbacks map[uint32]func()
	reducerCallbacks      map[uint32]func(protocol.ServerMessage)
	oneOffCallbacks       map[uint32]chan any
}

func (c *dbConnection) Identity() types.Identity        { return c.identity }
func (c *dbConnection) ConnectionId() types.ConnectionId { return c.connectionID }
func (c *dbConnection) Token() string                    { return c.token }
func (c *dbConnection) IsActive() bool                   { return c.active.Load() }
func (c *dbConnection) Cache() cache.ClientCache         { return c.cache }

func (c *dbConnection) RegisterTable(def cache.TableDef) {
	c.cache.RegisterTable(def)
}

func (c *dbConnection) CallReducer(reducer string, args bsatn.Serializable) error {
	var encoded []byte
	if args != nil {
		w := bsatn.NewWriter(64)
		args.WriteBsatn(w)
		encoded = w.Bytes()
	}
	c.commands <- &command{
		typ:     cmdCallReducer,
		reducer: reducer,
		args:    encoded,
	}
	return nil
}

func (c *dbConnection) OneOffQuery(query string) ([][]byte, error) {
	result := make(chan any, 1)
	c.commands <- &command{
		typ:     cmdOneOffQuery,
		queries: []string{query},
		result:  result,
	}
	resp := <-result
	switch r := resp.(type) {
	case oneOffResult:
		return r.rows, r.err
	default:
		return nil, &ProtocolError{Message: "unexpected one-off query result type"}
	}
}

func (c *dbConnection) Subscribe(queries ...string) SubscriptionBuilder {
	return &subscriptionBuilder{
		conn:    c,
		queries: queries,
	}
}

func (c *dbConnection) Disconnect() error {
	c.commands <- &command{typ: cmdDisconnect}
	return nil
}

// Run is the single-goroutine event loop. All state access on
// subscriptionCallbacks and reducerCallbacks happens here.
func (c *dbConnection) Run(ctx context.Context) error {
	c.active.Store(true)
	defer c.active.Store(false)

	c.subscriptionCallbacks = make(map[uint32]func())
	c.reducerCallbacks = make(map[uint32]func(protocol.ServerMessage))
	c.oneOffCallbacks = make(map[uint32]chan any)

	// readCtx is derived from context.Background() rather than the caller's
	// ctx so that context cancellation does not kill the TCP connection before
	// the WebSocket close handshake can complete. Close() sends the close
	// frame through the still-active reader, then internally closes the TCP
	// connection, which causes Read() to return and readLoop to exit.
	readCtx, readCancel := context.WithCancel(context.Background())
	defer readCancel()

	readDone := make(chan struct{})
	go func() {
		defer close(readDone)
		c.readLoop(readCtx)
	}()

	// shutdown performs a clean WebSocket close handshake and waits for
	// readLoop to exit, preventing goroutine leaks.
	shutdown := func(disconnectErr error) {
		c.ws.Close()  // Sends close frame, waits for peer response
		readCancel()  // Belt-and-suspenders: cancel readCtx
		<-readDone    // Wait for readLoop goroutine to exit
		if c.onDisconnect != nil {
			c.onDisconnect(c, disconnectErr)
		}
	}

	for {
		select {
		case <-ctx.Done():
			shutdown(ctx.Err())
			return ctx.Err()

		case cmd := <-c.commands:
			if err := c.handleCommand(ctx, cmd); err != nil {
				return err
			}
			if cmd.typ == cmdDisconnect {
				shutdown(nil)
				return nil
			}

		case msg := <-c.incoming:
			c.handleIncoming(msg)

		case <-readDone:
			// readLoop exited unexpectedly (server closed, network error).
			if c.onDisconnect != nil {
				c.onDisconnect(c, &ConnectionError{Message: "connection lost"})
			}
			return &ConnectionError{Message: "connection lost"}
		}
	}
}

func (c *dbConnection) readLoop(ctx context.Context) {
	for {
		data, err := c.ws.Recv(ctx)
		if err != nil {
			return
		}
		c.incoming <- data
	}
}

func (c *dbConnection) handleCommand(ctx context.Context, cmd *command) error {
	switch cmd.typ {
	case cmdCallReducer:
		reqID := c.nextRequestID.Add(1)
		msg := &protocol.CallReducer{
			RequestID: reqID,
			Flags:     0,
			Reducer:   cmd.reducer,
			Args:      cmd.args,
		}
		return c.sendMessage(ctx, msg)

	case cmdSubscribe:
		if cmd.onApplied != nil {
			c.subscriptionCallbacks[cmd.querySetID] = cmd.onApplied
		}
		reqID := c.nextRequestID.Add(1)
		msg := &protocol.Subscribe{
			RequestID:    reqID,
			QuerySetID:   cmd.querySetID,
			QueryStrings: cmd.queries,
		}
		return c.sendMessage(ctx, msg)

	case cmdUnsubscribe:
		reqID := c.nextRequestID.Add(1)
		msg := &protocol.Unsubscribe{
			RequestID:  reqID,
			QuerySetID: cmd.querySetID,
		}
		return c.sendMessage(ctx, msg)

	case cmdOneOffQuery:
		reqID := c.nextRequestID.Add(1)
		if cmd.result != nil {
			c.oneOffCallbacks[reqID] = cmd.result
		}
		msg := &protocol.OneOffQuery{
			RequestID:   reqID,
			QueryString: cmd.queries[0],
		}
		return c.sendMessage(ctx, msg)

	case cmdDisconnect:
		// Close and cleanup handled by shutdown() in Run().
		return nil
	}
	return nil
}

func (c *dbConnection) sendMessage(ctx context.Context, msg protocol.ClientMessage) error {
	data := bsatn.Encode(msg)
	return c.ws.Send(ctx, data)
}

func (c *dbConnection) handleIncoming(data []byte) {
	decompressed, err := protocol.DecompressMessage(data)
	if err != nil {
		return
	}

	r := bsatn.NewReader(decompressed)
	msg, err := protocol.ReadServerMessage(r)
	if err != nil {
		return
	}

	switch m := msg.(type) {
	case *protocol.InitialConnection:
		c.identity = m.Identity
		c.connectionID = m.ConnectionID
		c.token = m.Token
		if c.onConnect != nil {
			c.onConnect(c, m.Identity, m.Token)
		}

	case *protocol.SubscribeApplied:
		c.cache.ApplySubscribeApplied(&m.Rows)
		if cb, ok := c.subscriptionCallbacks[m.QuerySetID]; ok {
			cb()
		}

	case *protocol.TransactionUpdate:
		c.cache.ApplyTransactionUpdate(m)

	case *protocol.SubscriptionError:
		// Log or report subscription error; for now, remove the callback.
		delete(c.subscriptionCallbacks, m.QuerySetID)

	case *protocol.OneOffQueryResult:
		if ch, ok := c.oneOffCallbacks[m.RequestID]; ok {
			if m.ResultErr != "" {
				ch <- oneOffResult{err: &ProtocolError{Message: m.ResultErr}}
			} else {
				var rows [][]byte
				if m.ResultOk != nil {
					for _, tableRows := range m.ResultOk.Tables {
						if tableRows.Rows != nil {
							rows = append(rows, tableRows.Rows.Rows()...)
						}
					}
				}
				ch <- oneOffResult{rows: rows}
			}
			delete(c.oneOffCallbacks, m.RequestID)
		}

	case *protocol.ReducerResult:
		if cb, ok := c.reducerCallbacks[m.RequestID]; ok {
			cb(msg)
			delete(c.reducerCallbacks, m.RequestID)
		}
		// Apply transaction update from successful reducer outcomes.
		switch outcome := m.Result.(type) {
		case *protocol.ReducerOk:
			if outcome.TransactionUpdate != nil {
				c.cache.ApplyTransactionUpdate(outcome.TransactionUpdate)
			}
		}
	}
}

// subscriptionBuilder implements SubscriptionBuilder.
type subscriptionBuilder struct {
	conn      *dbConnection
	queries   []string
	onApplied func()
}

func (sb *subscriptionBuilder) OnApplied(fn func()) SubscriptionBuilder {
	sb.onApplied = fn
	return sb
}

func (sb *subscriptionBuilder) Build() (SubscriptionHandle, error) {
	qsID := sb.conn.nextQuerySetID.Add(1)

	// Send the subscribe command with the onApplied callback so the Run()
	// event loop can register it in its own goroutine (no data races).
	sb.conn.commands <- &command{
		typ:        cmdSubscribe,
		queries:    sb.queries,
		querySetID: qsID,
		onApplied:  sb.onApplied,
	}

	return &subscriptionHandle{
		conn:       sb.conn,
		querySetID: qsID,
		active:     true,
	}, nil
}

// subscriptionHandle implements SubscriptionHandle.
type subscriptionHandle struct {
	conn       *dbConnection
	querySetID uint32
	active     bool
}

func (sh *subscriptionHandle) Unsubscribe() error {
	sh.active = false
	sh.conn.commands <- &command{
		typ:        cmdUnsubscribe,
		querySetID: sh.querySetID,
	}
	return nil
}

func (sh *subscriptionHandle) IsActive() bool {
	return sh.active
}

// Ensure interfaces are satisfied at compile time.
var (
	_ DbConnectionBuilder = (*dbConnectionBuilder)(nil)
	_ DbConnection        = (*dbConnection)(nil)
	_ SubscriptionBuilder = (*subscriptionBuilder)(nil)
	_ SubscriptionHandle  = (*subscriptionHandle)(nil)
	_ error               = (*ConnectionError)(nil)
	_ error               = (*ReducerError)(nil)
	_ error               = (*SubscriptionError)(nil)
	_ error               = (*ProtocolError)(nil)
)
