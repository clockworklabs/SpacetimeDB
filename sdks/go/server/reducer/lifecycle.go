package reducer

// Lifecycle identifies lifecycle reducer types.
type Lifecycle uint8

const (
	LifecycleInit             Lifecycle = 0
	LifecycleClientConnected    Lifecycle = 1
	LifecycleClientDisconnected Lifecycle = 2
)

func (l Lifecycle) String() string {
	switch l {
	case LifecycleInit:
		return "__init__"
	case LifecycleClientConnected:
		return "__identity_connected__"
	case LifecycleClientDisconnected:
		return "__identity_disconnected__"
	default:
		return "unknown"
	}
}

// InitReducerFunc is the signature for the __init__ lifecycle reducer.
type InitReducerFunc func(ctx ReducerContext)

// ClientConnectedReducerFunc is the signature for the __identity_connected__ lifecycle reducer.
type ClientConnectedReducerFunc func(ctx ReducerContext)

// ClientDisconnectedReducerFunc is the signature for the __identity_disconnected__ lifecycle reducer.
type ClientDisconnectedReducerFunc func(ctx ReducerContext)
