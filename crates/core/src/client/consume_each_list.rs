use bytes::Bytes;
use spacetimedb_client_api_messages::websocket::{v1 as ws_v1, v2 as ws_v2};

/// Moves each buffer in `self` into a closure.
pub trait ConsumeEachBuffer {
    /// Consumes `self`, moving each `Bytes` buffer in `self` into the closure `each`.
    fn consume_each_list(self, each: &mut impl FnMut(Bytes));
}

impl ConsumeEachBuffer for ws_v2::QueryRows {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.tables
            .into_vec()
            .into_iter()
            .for_each(|x| x.rows.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for ws_v2::TableUpdateRows {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        match self {
            ws_v2::TableUpdateRows::EventTable(x) => x.events.consume_each_list(each),
            ws_v2::TableUpdateRows::PersistentTable(x) => {
                x.inserts.consume_each_list(each);
                x.deletes.consume_each_list(each);
            }
        }
    }
}

impl ConsumeEachBuffer for ws_v2::TransactionUpdate {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.query_sets
            .into_vec()
            .into_iter()
            .flat_map(|x| x.tables.into_vec())
            .flat_map(|x| x.rows.into_vec())
            .for_each(|x| x.consume_each_list(each));
    }
}

impl<T: ConsumeEachBuffer> ConsumeEachBuffer for Option<T> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        if let Some(v) = self {
            v.consume_each_list(each);
        }
    }
}

impl ConsumeEachBuffer for ws_v2::ServerMessage {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        use ws_v2::ServerMessage::*;
        match self {
            SubscribeApplied(x) => x.rows.consume_each_list(each),
            OneOffQueryResult(x) => x.result.ok().consume_each_list(each),
            UnsubscribeApplied(x) => x.rows.consume_each_list(each),
            SubscriptionError(_) | InitialConnection(_) | ProcedureResult(_) => {}
            TransactionUpdate(x) => x.consume_each_list(each),
            ReducerResult(x) => {
                if let ws_v2::ReducerOutcome::Ok(ro) = x.result {
                    ro.transaction_update.consume_each_list(each);
                }
            }
        }
    }
}

impl ConsumeEachBuffer for ws_v1::ServerMessage<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        use ws_v1::ServerMessage::*;
        match self {
            InitialSubscription(x) => x.database_update.consume_each_list(each),
            TransactionUpdate(x) => x.status.consume_each_list(each),
            TransactionUpdateLight(x) => x.update.consume_each_list(each),
            IdentityToken(_) | ProcedureResult(_) | SubscriptionError(_) => {}
            OneOffQueryResponse(x) => x.consume_each_list(each),
            SubscribeApplied(x) => x.rows.table_rows.consume_each_list(each),
            UnsubscribeApplied(x) => x.rows.table_rows.consume_each_list(each),
            SubscribeMultiApplied(x) => x.update.consume_each_list(each),
            UnsubscribeMultiApplied(x) => x.update.consume_each_list(each),
        }
    }
}

impl ConsumeEachBuffer for ws_v1::OneOffQueryResponse<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        Vec::from(self.tables)
            .into_iter()
            .for_each(|x| x.rows.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for ws_v1::UpdateStatus<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        match self {
            Self::Committed(x) => x.consume_each_list(each),
            Self::Failed(_) | ws_v1::UpdateStatus::OutOfEnergy => {}
        }
    }
}

impl ConsumeEachBuffer for ws_v1::DatabaseUpdate<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.tables.into_iter().for_each(|x| x.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for ws_v1::TableUpdate<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.updates.into_iter().for_each(|x| x.consume_each_list(each));
    }
}

impl ConsumeEachBuffer for ws_v1::CompressableQueryUpdate<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        match self {
            Self::Uncompressed(x) => x.consume_each_list(each),
            Self::Brotli(bytes) | Self::Gzip(bytes) => each(bytes),
        }
    }
}

impl ConsumeEachBuffer for ws_v1::QueryUpdate<ws_v1::BsatnFormat> {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        self.deletes.consume_each_list(each);
        self.inserts.consume_each_list(each);
    }
}

impl ConsumeEachBuffer for ws_v1::BsatnRowList {
    fn consume_each_list(self, each: &mut impl FnMut(Bytes)) {
        let (_, buffer) = self.into_inner();
        each(buffer);
    }
}
