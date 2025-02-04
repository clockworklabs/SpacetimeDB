
using System;
using System.Collections.Generic;
using System.Data;
using System.Diagnostics;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text.RegularExpressions;

namespace SpacetimeDB
{
    /// <summary>
    /// Helper class for maintaining a pool of open subscriptions.
    /// 
    /// You construct one of these with a way to create SubscriptionBuilders,
    /// register a callback with OnSubscriptionsSteady, and then call SetSubscriptions as often as you want.
    /// 
    /// Your callback will be invoked as soon as all subscriptions reach a steady state: needed subscriptions are open (or errored)
    /// and unneeded subscriptions are closed (or errored).
    /// 
    /// You don't need to wait for OnSubscriptionsSteady to call SetSubscriptions again.
    /// 
    /// Subscriptions may error, which will result in a message being logged to the console.
    /// You can still receive callbacks with OnSubscriptionsSteady. Data from errored subscriptions will not be present in the
    /// DbConnection.Db.
    ///
    /// This class, unlike most classes in the SDK, is thread-safe, because it's nice to be paranoid sometimes.
    /// </summary>
    public class SubscriptionHelper<EventContext, DbConnection>
        where EventContext : IEventContext
    {
        /// <summary>
        /// State flow chart:
        /// <c>
        ///             │
        ///             ↓
        ///          [Pending] ←─────────────┐
        ///          │   │   │               │
        ///          │   │   │               │
        ///          ↓   │   ↓               │
        ///    [Ready]───┼──→[Errored]       │
        ///      │       │     ↑  ↑          │
        ///      │       │     │  │          │
        ///      ↓       ↓     │  │          │
        ///      [Closing] ────┘  │          │
        ///       │     │         │          │
        ///       │     │         │          │
        ///       │     ↓         │          │
        ///       │  [ClosingThenReopening]-─┘
        ///       │
        ///       ↓
        /// </c>
        /// 
        /// We assume that OnError can be received at any time. This results in a subscription
        /// being permanently assigned the Errored state. We do not attempt to resume Errored
        /// subscriptions.
        /// </summary>
        enum State
        {
            // Waiting for OnApply or OnError.
            Pending,
            // Received OnApply.
            Ready,
            // Received OnError.
            Errored,
            // Is going to close. Delete when closed.
            // May still be waiting on a SubscribeApplied, which we should discard.
            Closing,
            // Is going to close. Re-open when closed.
            // May still be waiting on a SubscribeApplied, which we should discard.
            ClosingThenReopening
        }

        public delegate SubscriptionBuilder<EventContext> SubscriptionBuilderBuilder();

        public event Action? OnSubscriptionsSteady;

        private readonly SubscriptionBuilderBuilder subscriptionBuilderBuilder;
        private readonly Dictionary<string, (SubscriptionHandle<EventContext> subscription, State state)> subscriptions = new();
        private readonly Dictionary<State, uint> stateCounts = new() { { State.Pending, 0 }, { State.Ready, 0 }, { State.Errored, 0 }, { State.Closing, 0 }, { State.ClosingThenReopening, 0 } };

        public SubscriptionHelper(SubscriptionBuilderBuilder subscriptionBuilderBuilder)
        {
            this.subscriptionBuilderBuilder = subscriptionBuilderBuilder;
        }

        private readonly Dictionary<State, uint> actualStateCounts = new();
        internal void CheckInvariants()
        {
#if DEBUG
            // locks are reentrant, so it's safe to call this while already holding the lock.
            lock (this)
            {
                actualStateCounts[State.Pending] = 0;
                actualStateCounts[State.Ready] = 0;
                actualStateCounts[State.Errored] = 0;
                actualStateCounts[State.Closing] = 0;
                actualStateCounts[State.ClosingThenReopening] = 0;

                foreach (var (query, (subscription, state)) in subscriptions)
                {
                    actualStateCounts[state]++;

                    // Our understanding of the state and the SDK's understanding should be the same:
                    switch (state)
                    {
                        case State.Pending:
                            Debug.Assert(!subscription.IsActive && !subscription.IsEnded);
                            break;

                        case State.Ready:
                            Debug.Assert(subscription.IsActive && !subscription.IsEnded);
                            break;
                        case State.Closing:
                        case State.ClosingThenReopening:
                            // It's possible (?) that the subscription was closed before it received OnApplied,
                            // so we don't know what IsActive should be.
                            Debug.Assert(!subscription.IsEnded);
                            break;

                        case State.Errored:
                            Debug.Assert(!subscription.IsActive && subscription.IsEnded);
                            break;
                    }
                }
                Debug.Assert(stateCounts == actualStateCounts, "State counts are in sync");
            }
#endif
        }

        private void CreateSubscription(string query)
        {
            lock (this)
            {
                Debug.Assert(!subscriptions.ContainsKey(query), "Subscription already exists for query", query);
                var subscriptionBuilder = subscriptionBuilderBuilder();

                SubscriptionHandle<EventContext> newSubscription = subscriptionBuilder.OnApplied((EventContext ctx) =>
                {
                    lock (this)
                    {
                        if (!subscriptions.TryGetValue(query, out var result))
                        {
                            // This could happen if we got an OnClose callback before an OnApplied callback.
                            // In this case, do nothing?
                            return;
                        }
                        var (subscription, state) = result;
                        if (state == State.Pending)
                        {
                            SetState(query, State.Ready);
                        }
                        else if (state == State.Errored || state == State.Ready)
                        {
                            Log.Warn($"Got unexpected OnApplied callback for subscription `{query}` in state `{state}`");
                        }
                        // state could also be: Closing, ClosingThenReopening.
                        // In these cases, leave things be.
                        CheckInvariants();
                        DecideAboutCallbacks();
                    }
                })
                .OnError((EventContext ctx) =>
                {
                    lock (this)
                    {
                        if (!subscriptions.TryGetValue(query, out var result))
                        {
                            // this could happen if we got an OnClose callback before an OnApplied callback.
                            // in this case, do nothing?
                            return;
                        }
                        var (subscription, state) = result;

                        SetState(query, State.Errored);

                        CheckInvariants();
                        DecideAboutCallbacks();
                    }
                })
                .Subscribe(query);

                subscriptions[query] = (newSubscription, State.Pending);
                stateCounts[State.Pending]++;
            }
        }

        /// <summary>
        /// Close a subscription.
        /// The subscription must be in the "Pending" or "Ready" states.
        /// </summary>
        /// <param name="query"></param>
        private void CloseSubscription(string query)
        {
            lock (this)
            {
                Debug.Assert(subscriptions.ContainsKey(query), "Can't close non-open subscription");

                var (subscription, state) = subscriptions[query];

                Debug.Assert(state == State.Pending || state == State.Ready, "Can't close subscription in unexpected state");

                subscription.UnsubscribeThen((EventContext ctx) =>
                {
                    lock (this)
                    {
                        var (_, state) = subscriptions[query];

                        if (state == State.Errored)
                        {
                            // leave it be, we need to remember it's a problem.
                            return;
                        }

                        RemoveSubscription(query);

                        if (state == State.ClosingThenReopening)
                        {
                            CreateSubscription(query);
                        }

                    }
                });
            }
        }

        /// <summary>
        /// Decide whether or not to invoke OnSubscriptionsSteady based on state counts.
        /// </summary>
        private void DecideAboutCallbacks()
        {
            lock (this)
            {
                if (stateCounts[State.Pending] == 0 && stateCounts[State.Closing] == 0 && stateCounts[State.ClosingThenReopening] == 0)
                {
                    OnSubscriptionsSteady?.Invoke();
                }
            }
        }

        /// <summary>
        /// Set the state for a subscription.
        /// It must be present in the subscriptions dictionary.
        /// </summary>
        /// <param name="query"></param>
        /// <param name="newState"></param>
        private void SetState(string query, State newState)
        {
            lock (this)
            {
                var (subscription, oldState) = subscriptions[query];

                stateCounts[oldState]--;
                stateCounts[newState]++;
                subscriptions[query] = (subscription, newState);
            }
        }

        /// <summary>
        /// Remove a subscription.
        /// The subscription must be present and in the Closing or ClosingThenReopening states.
        /// </summary>
        /// <param name="query"></param>
        private void RemoveSubscription(string query)
        {
            lock (this)
            {
                if (!subscriptions.ContainsKey(query))
                {
                    Log.Warn($"Removing missing subscription? `{query}`");
                    return;
                }
                var (_, state) = subscriptions[query];
                Debug.Assert(state == State.Closing);
                stateCounts[state]--;
            }
        }

        public void SetSubscriptions(List<string> queries)
        {
            lock (this)
            {
                var queriesSorted = new SortedSet<string>(queries);

                // Create new queries
                foreach (var query in queriesSorted)
                {
                    if (subscriptions.TryGetValue(query, out var result))
                    {
                        var (subscription, state) = result;
                        if (state == State.Closing)
                        {
                            SetState(query, State.ClosingThenReopening);
                        }
                        // in any other case, we're good:
                        // Pending: fine
                        // Ready: fine
                        // Errored: don't try to reopen it
                        // ClosingThenReopening: fine
                    }
                    else
                    {
                        CreateSubscription(query);
                    }
                }

                // Remove dead queries
                foreach (var (query, (_, state)) in subscriptions.OrderBy(kv => kv.Key))
                {
                    if (!queriesSorted.Contains(query) && state != State.Errored)
                    {
                        CloseSubscription(query);
                    }
                }
                // make sure to call this before we exit the lock.
                CheckInvariants();
            }
        }
    }
}