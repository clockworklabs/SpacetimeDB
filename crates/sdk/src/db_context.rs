use crate::{Address, Identity};

pub trait DbContext {
    type DbView;
    fn db(&self) -> &Self::DbView;
    type Reducers;
    fn reducers(&self) -> &Self::Reducers;

    fn is_active(&self) -> bool;

    fn disconnect(&self) -> anyhow::Result<()>;

    type SubscriptionBuilder;
    fn subscription_builder(&self) -> Self::SubscriptionBuilder;

    fn identity(&self) -> Identity;
    fn address(&self) -> Address;
}
