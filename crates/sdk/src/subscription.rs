use crate::{
    db_connection::{DbContextImpl, PendingMutation},
    spacetime_module::{SpacetimeModule, SubscriptionHandle},
};
use spacetimedb_data_structures::map::HashMap;
use std::sync::atomic::AtomicU32;

// TODO: Rewrite for subscription manipulation, once we get that.
// Currently race conditions abound, as you may resubscribe before the prev sub was applied,
// clobbering your previous callback.

pub struct SubscriptionManager<M: SpacetimeModule> {
    subscriptions: HashMap<u32, SubscribedQuery<M>>,
}

impl<M: SpacetimeModule> Default for SubscriptionManager<M> {
    fn default() -> Self {
        Self {
            subscriptions: HashMap::default(),
        }
    }
}

pub(crate) type OnAppliedCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;
pub(crate) type OnErrorCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;

impl<M: SpacetimeModule> SubscriptionManager<M> {
    pub(crate) fn register_subscription(
        &mut self,
        sub_id: u32,
        on_applied: Option<OnAppliedCallback<M>>,
        on_error: Option<OnErrorCallback<M>>,
    ) {
        self.subscriptions
            .try_insert(
                sub_id,
                SubscribedQuery {
                    on_applied,
                    on_error,
                    is_applied: false,
                },
            )
            .unwrap_or_else(|_| unreachable!("Duplicate subscription id {sub_id}"));
    }
    pub(crate) fn subscription_applied(&mut self, ctx: &M::EventContext, sub_id: u32) {
        let sub = self.subscriptions.get_mut(&sub_id).unwrap();
        sub.is_applied = true;
        if let Some(callback) = sub.on_applied.take() {
            callback(ctx);
        }
    }
}

struct SubscribedQuery<M: SpacetimeModule> {
    on_applied: Option<OnAppliedCallback<M>>,
    #[allow(unused)]
    on_error: Option<OnErrorCallback<M>>,
    is_applied: bool,
}

pub struct SubscriptionBuilder<M: SpacetimeModule> {
    on_applied: Option<OnAppliedCallback<M>>,
    on_error: Option<OnErrorCallback<M>>,
    conn: DbContextImpl<M>,
}

impl<M: SpacetimeModule> SubscriptionBuilder<M> {
    #[doc(hidden)]
    pub fn new(imp: &DbContextImpl<M>) -> Self {
        Self {
            on_applied: None,
            on_error: None,
            conn: imp.clone(),
        }
    }

    pub fn on_applied(mut self, callback: impl FnOnce(&M::EventContext) + Send + 'static) -> Self {
        self.on_applied = Some(Box::new(callback));
        self
    }

    pub fn on_error(mut self, callback: impl FnOnce(&M::EventContext) + Send + 'static) -> Self {
        self.on_error = Some(Box::new(callback));
        self
    }

    pub fn subscribe(self, queries: Vec<String>) -> M::SubscriptionHandle {
        static NEXT_SUB_ID: AtomicU32 = AtomicU32::new(0);

        let sub_id = NEXT_SUB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let Self {
            on_applied,
            on_error,
            conn,
        } = self;
        conn.pending_mutations_send
            .unbounded_send(PendingMutation::Subscribe {
                on_applied,
                on_error,
                queries,
                sub_id,
            })
            .unwrap();
        M::SubscriptionHandle::new(SubscriptionHandleImpl { conn, sub_id })
    }
}

#[doc(hidden)]
pub struct SubscriptionHandleImpl<M: SpacetimeModule> {
    conn: DbContextImpl<M>,
    #[allow(unused)]
    sub_id: u32,
}

impl<M: SpacetimeModule> SubscriptionHandleImpl<M> {
    pub fn is_ended(&self) -> bool {
        // When a subscription ends, we remove its `SubscribedQuery` from the `SubscriptionManager`.
        // So, to check if a subscription has ended, we check if the entry is present.
        // TODO: Note that we never end a subscription currently.
        // This will change with the implementation of the subscription management proposal.
        !self
            .conn
            .inner
            .lock()
            .unwrap()
            .subscriptions
            .subscriptions
            .contains_key(&self.sub_id)
    }
    pub fn is_active(&self) -> bool {
        // A subscription is active if:
        // - It has not yet ended, i.e. is still present in the `SubscriptionManager`.
        // - It has been applied.
        self.conn
            .inner
            .lock()
            .unwrap()
            .subscriptions
            .subscriptions
            .get(&self.sub_id)
            .map(|sub| sub.is_applied)
            .unwrap_or(false)
    }
    pub fn unsubscribe(self) -> anyhow::Result<()> {
        self.unsubscribe_then(|_| {})
    }
    pub fn unsubscribe_then(self, _on_end: impl FnOnce(&M::EventContext) + Send + 'static) -> anyhow::Result<()> {
        todo!()
    }
}
