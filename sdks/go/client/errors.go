package client

import "fmt"

// ConnectionError represents a WebSocket connection failure.
type ConnectionError struct {
	Message string
	Err     error
}

func (e *ConnectionError) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("connection error: %s: %v", e.Message, e.Err)
	}
	return fmt.Sprintf("connection error: %s", e.Message)
}

func (e *ConnectionError) Unwrap() error { return e.Err }

// ReducerError represents a reducer invocation failure.
type ReducerError struct {
	ReducerName string
	Message     string
}

func (e *ReducerError) Error() string {
	return fmt.Sprintf("reducer %s error: %s", e.ReducerName, e.Message)
}

// SubscriptionError represents a subscription failure.
type SubscriptionError struct {
	QuerySetID uint32
	Message    string
}

func (e *SubscriptionError) Error() string {
	return fmt.Sprintf("subscription %d error: %s", e.QuerySetID, e.Message)
}

// ProtocolError represents a BSATN protocol decoding or encoding failure.
type ProtocolError struct {
	Message string
	Err     error
}

func (e *ProtocolError) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("protocol error: %s: %v", e.Message, e.Err)
	}
	return fmt.Sprintf("protocol error: %s", e.Message)
}

func (e *ProtocolError) Unwrap() error { return e.Err }
