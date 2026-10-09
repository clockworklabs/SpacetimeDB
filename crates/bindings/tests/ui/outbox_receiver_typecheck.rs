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
        pub target: spacetimedb::Identity,
        pub body: String,
    }

    #[allow(non_camel_case_types)]
    pub struct receive_ping;

    impl spacetimedb::rt::RemoteReducer for receive_ping {
        const NAME: &'static str = "receive_ping";
        const ARG_NAMES: &'static [&'static str] = &["target", "body"];
        const SIGNATURE_HASH: &'static str = "test-signature";
        type Args = ReceivePingArgs;
    }
}

#[spacetimedb::table(accessor = outbound_ping, outbox(receiver::receive_ping))]
pub struct OutboundPing {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: String,
    #[target]
    target: u8,
}

fn main() {}
