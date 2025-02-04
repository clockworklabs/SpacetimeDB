
using System;
using System.Collections.Generic;
using System.Data;
using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Text.RegularExpressions;

namespace SpacetimeDB
{

    /// <summary>
    /// Helper class for maintaining a pool of open subscriptions.
    ///
    /// This class, unlike most classes in the SDK, is thread-safe, because it's nice to be paranoid sometimes.
    /// </summary>
    public class SubscriptionHelper<EventContext, DbConnection>
        where EventContext : IEventContext
    {
        public delegate SubscriptionBuilder<EventContext> SubscriptionBuilderBuilder();

        SubscriptionBuilderBuilder subscriptionBuilderBuilder;

        enum State
        {
            // Waiting for response to our Subscribe message..
            Pending,
            // Has already connected.
            Ready,
            // Has returned an error. Remember this and don't try to resubscribe.
            Errored,
            // Is going to close. Delete when closed.
            PendingClose,
            // Is going to close. Re-open when closed.
            PendingCloseAndReopen
        }

        Dictionary<string, SubscriptionHandle<EventContext>> openSubscriptions = new();

        Dictionary<SubscriptionHandle<EventContext>, State> states = new();
        Dictionary<State, uint> stateCounts = new() { { State.Pending, 0 }, { State.Ready, 0 }, { State.Errored, 0 }, { State.PendingClose, 0 }, { State.PendingCloseAndReopen, 0 } };

        public SubscriptionHelper(SubscriptionBuilderBuilder subscriptionBuilderBuilder)
        {
            this.subscriptionBuilderBuilder = subscriptionBuilderBuilder;
        }

        internal void CheckInvariants()
        {
#if DEBUG
            Dictionary<State, uint> actualStateCounts = new() { { State.Pending, 0 }, { State.Ready, 0 }, { State.Errored, 0 }, { State.PendingClose, 0 }, { State.PendingCloseAndReopen, 0 } };

            // locks are reentrant, so it's safe to call this while already holding the lock.
            lock (this)
            {
                foreach (var (query, subscription) in openSubscriptions)
                {
                    Debug.Assert(states.ContainsKey(subscription), "All subscriptions should have a state");
                    var state = states[subscription];
                    actualStateCounts[state]++;

                    // Our understanding of the state and the SDK's understanding should be the same:
                    switch (state)
                    {
                        case State.Pending:
                            Debug.Assert(!subscription.IsActive && !subscription.IsEnded);
                            break;

                        case State.Ready:
                        case State.PendingClose:
                        case State.PendingCloseAndReopen:
                            Debug.Assert(subscription.IsActive && !subscription.IsEnded);
                            break;

                        case State.Errored:
                            Debug.Assert(!subscription.IsActive && subscription.IsEnded);
                            break;
                    }
                }
                Debug.Assert(openSubscriptions.Count == states.Count, "Each subscription has a state");
                Debug.Assert(stateCounts == actualStateCounts, "State counts are in sync");
            }
#endif
        }

        private void SetState(SubscriptionHandle<EventContext> subscription, State newState)
        {
            lock (this)
            {
                if (states.TryGetValue(subscription, out var oldState))
                {
                    stateCounts[oldState]--;
                }
                stateCounts[newState]++;
                states[subscription] = newState;
            }
        }

        private void RemoveSubscription(string query)
        {
            lock (this)
            {
                if (!openSubscriptions.TryGetValue(query, out var subscription))
                {
                    Log.Warn("Removing non-present subscription");
                    return;
                }
                if (states.TryGetValue(subscription, out var oldState))
                {
                    stateCounts[oldState]--;
                }
                states.Remove(subscription);
                openSubscriptions.Remove(query);
            }

        }

        public void SetSubscriptions(List<string> queries)
        {
            var queriesSorted = new SortedSet<string>(queries);

            lock (this)
            {
                foreach (var query in queriesSorted)
                {
                    if (openSubscriptions.TryGetValue(query, out var subscription))
                    {
                        if (states[subscription] == State.PendingClose)
                        {
                            SetState(subscription, State.PendingCloseAndReopen);
                        }
                        // in any other case, the subscription is fine, and we can leave it be.
                    }
                    else
                    {
                        var subscriptionBuilder = subscriptionBuilderBuilder();

                        // This will never actually be null, as the builder is guaranteed to return a handle
                        // before any callbacks are invoked. (The callbacks wait for the network!)
                        SubscriptionHandle<EventContext>? newSubscription = null;
                        newSubscription = subscriptionBuilder.OnApplied((EventContext ctx) =>
                        {
                            // we need to re-lock this in the callback.
                            lock (this)
                            {
                                SetState(newSubscription!, State.Ready);
                                CheckInvariants();
                                // TODO: decide whether to invoke callback
                            }
                        })
                        .OnError((EventContext ctx) =>
                        {
                            lock (this)
                            {
                                SetState(newSubscription!, State.Errored);
                                CheckInvariants();
                                // TODO: decide whether to invoke callback
                            }
                        })
                        .Subscribe(query);

                        openSubscriptions[query] = newSubscription;
                        SetState(newSubscription, State.Pending);
                    }
                }

                // TODO: check for dead queries

                // make sure to call this before we exit the lock.
                CheckInvariants();
            }
        }
    }
}