mod receiver {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct Identity(pub spacetimedb::Identity);

    impl spacetimedb::SpacetimeType for Identity {
        fn make_type<S: spacetimedb::spacetimedb_lib::sats::typespace::TypespaceBuilder>(
            typespace: &mut S,
        ) -> spacetimedb::spacetimedb_lib::AlgebraicType {
            <spacetimedb::Identity as spacetimedb::SpacetimeType>::make_type(typespace)
        }
    }

    impl spacetimedb::Serialize for Identity {
        fn serialize<S: spacetimedb::spacetimedb_lib::ser::Serializer>(
            &self,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            <spacetimedb::Identity as spacetimedb::Serialize>::serialize(&self.0, serializer)
        }
    }

    impl<'de> spacetimedb::Deserialize<'de> for Identity {
        fn deserialize<D: spacetimedb::spacetimedb_lib::de::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Self, D::Error> {
            <spacetimedb::Identity as spacetimedb::Deserialize>::deserialize(deserializer).map(Self)
        }
    }

    #[derive(spacetimedb::Serialize, spacetimedb::Deserialize)]
    pub struct ReceivePingArgs {
        pub body: String,
    }

    #[allow(non_camel_case_types)]
    pub struct receive_ping;

    impl spacetimedb::rt::RemoteReducer for receive_ping {
        const NAME: &'static str = "receive_ping";
        const ARG_NAMES: &'static [&'static str] = &["body"];
        const SIGNATURE_HASH: &'static str = "test-signature";
        type Args = ReceivePingArgs;
    }
}

#[spacetimedb::table(accessor = missing_pk, outbox(receiver::receive_ping))]
pub struct MissingPk {
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = non_auto_inc_pk, outbox(receiver::receive_ping))]
pub struct NonAutoIncPk {
    #[primary_key]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = wrong_pk_type, outbox(receiver::receive_ping))]
pub struct WrongPkType {
    #[primary_key]
    #[auto_inc]
    id: u32,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = scheduled_and_outbox, scheduled(scheduled), outbox(receiver::receive_ping))]
pub struct ScheduledAndOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::table(accessor = callback_outbox, outbox(receiver::receive_ping))]
pub struct CallbackOutbox {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[target]
    target: receiver::Identity,
    body: String,
}

#[spacetimedb::reducer(on_result(callback_outbox))]
pub fn bad_on_result(_ctx: &spacetimedb::ReducerContext, _row: CallbackOutbox) {}

fn main() {}
