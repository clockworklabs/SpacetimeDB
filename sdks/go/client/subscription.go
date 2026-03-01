package client

// SubscriptionBuilder configures and creates a subscription.
type SubscriptionBuilder interface {
	OnApplied(fn func()) SubscriptionBuilder
	Build() (SubscriptionHandle, error)
}

// SubscriptionHandle represents an active subscription.
type SubscriptionHandle interface {
	Unsubscribe() error
	IsActive() bool
}

// CallbackID identifies a registered callback.
type CallbackID uint64
