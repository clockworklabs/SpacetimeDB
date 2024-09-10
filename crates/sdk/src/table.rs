pub trait Table {
    type Row;

    type EventContext;

    fn count(&self) -> u64;

    fn iter(&self) -> impl Iterator<Item = Self::Row> + '_;

    type InsertCallbackId;
    fn on_insert(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + Sync + 'static,
    ) -> Self::InsertCallbackId;
    fn remove_on_insert(&self, callback: Self::InsertCallbackId);

    type DeleteCallbackId;
    fn on_delete(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row) + Send + Sync + 'static,
    ) -> Self::DeleteCallbackId;
    fn remove_on_delete(&self, callback: Self::DeleteCallbackId);
}

pub trait TableWithPrimaryKey: Table {
    type UpdateCallbackId;
    fn on_update(
        &self,
        callback: impl FnMut(&Self::EventContext, &Self::Row, &Self::Row) + Send + Sync + 'static,
    ) -> Self::UpdateCallbackId;
    fn remove_on_update(&self, callback: Self::UpdateCallbackId);
}
