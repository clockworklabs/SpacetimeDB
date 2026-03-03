package reducer

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ViewContext provides context to an authenticated view function.
type ViewContext interface {
	Sender() types.Identity
}

// AnonymousViewContext provides context to an anonymous view function.
// Anonymous views do not have access to the caller's identity.
type AnonymousViewContext interface {
	isAnonymousViewContext()
}

// ViewFunc is the internal dispatch signature for views.
// The args are raw BSATN bytes of the view's parameter product type.
// Returns the BSATN-encoded result to write to the sink.
type ViewFunc func(ctx any, args []byte) ([]byte, error)

// NewViewContext creates a ViewContext with the given sender identity.
func NewViewContext(sender types.Identity) ViewContext {
	return &viewContext{sender: sender}
}

// NewAnonymousViewContext creates an AnonymousViewContext.
func NewAnonymousViewContext() AnonymousViewContext {
	return &anonymousViewContext{}
}

type viewContext struct {
	sender types.Identity
}

func (c *viewContext) Sender() types.Identity { return c.sender }

type anonymousViewContext struct{}

func (c *anonymousViewContext) isAnonymousViewContext() {}
