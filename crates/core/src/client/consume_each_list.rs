use bytes::Bytes;
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, BsatnRowList, CompressableQueryUpdate, DatabaseUpdate, OneOffQueryResponse, QueryUpdate,
    ServerMessage, TableUpdate, UpdateStatus,
};

/// Moves each buffer in `self` into a closure.
pub trait ConsumeEachBuffer {
    /// Consumes `self`, moving each `Bytes` buffer in `self` into the closure `each`.
    fn consume_each_list(self, each: &mut impl FnMut(Bytes));
}

impl ConsumeEachBuffer for ServerMessage<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        use ServerMessage::*;
        match self {
            InitialSubscription(x) => x.database_update.consume_each_list(each),
            TransactionUpdate(x) => x.status.consume_each_list(each),
            TransactionUpdateLight(x) => x.update.consume_each_list(each),
            IdentityToken(_) | SubscriptionError(_) => {}
            OneOffQueryResponse(x) => x.consume_each_list(each),
            SubscribeApplied(x) => x.rows.table_rows.consume_each_list(each),
            UnsubscribeApplied(x) => x.rows.table_rows.consume_each_list(each),
            SubscribeMultiApplied(x) => x.update.consume_each_list(each),
            UnsubscribeMultiApplied(x) => x.update.consume_each_list(each),
        }
    }
}

impl ConsumeEachBuffer for OneOffQueryResponse<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        Vec::from(self.tables)
            .into_iter()
            .for_each(|x| x.rows.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for UpdateStatus<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        match self {
            Self::Committed(x) => x.consume_each_list(each),
            Self::Failed(_) | UpdateStatus::OutOfEnergy => {}
        }
    }
}

impl ConsumeEachBuffer for DatabaseUpdate<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.tables.into_iter().for_each(|x| x.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for TableUpdate<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.updates.into_iter().for_each(|x| x.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for CompressableQueryUpdate<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        match self {
            Self::Uncompressed(x) => x.consume_each_list(each),
            Self::Brotli(bytes) | Self::Gzip(bytes) => each(bytes),
        }
    }
}

impl ConsumeEachBuffer for QueryUpdate<BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.deletes.consume_each_list(each);
        self.inserts.consume_each_list(each);
    }
}

impl ConsumeEachBuffer for BsatnRowList {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        let (_, buffer) = self.into_inner();
        each(buffer);
    }
}
