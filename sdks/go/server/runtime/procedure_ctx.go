package runtime

import (
	"crypto/rand"
	"fmt"

	stdbhttp "github.com/clockworklabs/SpacetimeDB/sdks/go/server/http"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/reducer"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// procedureContext implements reducer.ProcedureContext.
type procedureContext struct {
	sender       types.Identity
	connectionId types.ConnectionId
	timestamp    types.Timestamp
	moduleId     types.Identity
	uuidCounter  uint32
}

var _ reducer.ProcedureContext = (*procedureContext)(nil)

// NewProcedureContext creates a ProcedureContext for procedure dispatch.
func NewProcedureContext(sender types.Identity, connId types.ConnectionId, ts types.Timestamp) reducer.ProcedureContext {
	return &procedureContext{
		sender:       sender,
		connectionId: connId,
		timestamp:    ts,
		moduleId:     types.NewIdentity(sys.GetIdentity()),
	}
}

func (c *procedureContext) Sender() types.Identity       { return c.sender }
func (c *procedureContext) ConnectionId() types.ConnectionId { return c.connectionId }
func (c *procedureContext) Timestamp() types.Timestamp    { return c.timestamp }
func (c *procedureContext) Identity() types.Identity      { return c.moduleId }

func (c *procedureContext) WithTx(fn func()) {
	if _, err := sys.ProcedureStartMutTx(); err != nil {
		panic(fmt.Sprintf("ProcedureContext.WithTx: start tx failed: %v", err))
	}
	defer func() {
		if r := recover(); r != nil {
			_ = sys.ProcedureAbortMutTx()
			panic(r)
		}
	}()
	fn()
	if err := sys.ProcedureCommitMutTx(); err != nil {
		panic(fmt.Sprintf("ProcedureContext.WithTx: commit failed: %v", err))
	}
}

func (c *procedureContext) TryWithTx(fn func() error) error {
	if _, err := sys.ProcedureStartMutTx(); err != nil {
		return fmt.Errorf("ProcedureContext.TryWithTx: start tx failed: %w", err)
	}

	var fnErr error
	func() {
		defer func() {
			if r := recover(); r != nil {
				_ = sys.ProcedureAbortMutTx()
				panic(r)
			}
		}()
		fnErr = fn()
	}()

	if fnErr != nil {
		_ = sys.ProcedureAbortMutTx()
		return fnErr
	}

	if err := sys.ProcedureCommitMutTx(); err != nil {
		return fmt.Errorf("ProcedureContext.TryWithTx: commit failed: %w", err)
	}
	return nil
}

func (c *procedureContext) SleepUntil(target types.Timestamp) {
	newTs := sys.ProcedureSleepUntil(target.Microseconds())
	c.timestamp = types.NewTimestamp(newTs)
}

func (c *procedureContext) HttpGet(uri string) (uint16, []byte, error) {
	return stdbhttp.Get(uri)
}

func (c *procedureContext) NewUuidV7() (types.Uuid, error) {
	var randomBytes [4]byte
	if _, err := rand.Read(randomBytes[:]); err != nil {
		return nil, fmt.Errorf("ProcedureContext.NewUuidV7: failed to generate random bytes: %w", err)
	}
	uuid, err := types.NewUuidV7(&c.uuidCounter, c.timestamp.Microseconds(), randomBytes)
	if err != nil {
		return nil, fmt.Errorf("ProcedureContext.NewUuidV7: %w", err)
	}
	return uuid, nil
}
