//! Can a per-item expansion, which cannot name the module-wide types, still produce today's per-item API?
//! It reaches them through `<Row as InModule>::Module`, and the aggregator implements `InModule` for the item.
use sdkstub::{ClientTableDef, InModule, SpacetimeModule};

/// "Per-item" code. It never names `RemoteModule`, `RemoteTables`, `Reducer` or `Ctx`.
pub mod item {
    pub struct Connected {
        pub x: u8,
    }
    pub struct ConnectedTableAccessor;
    impl sdkstub::ClientTableDef for ConnectedTableAccessor {
        type Row = Connected;
        const NAME: &'static str = "connected";
    }

    pub struct AddArgs {
        pub n: u8,
    }
    type M = <AddArgs as sdkstub::InModule>::Module;

    // (a) A local trait implemented for a projection type, like `impl PersonTableAccess for RemoteTables`.
    pub trait ConnectedTableAccess {
        fn connected(&self) -> u8;
    }
    impl ConnectedTableAccess for <M as sdkstub::SpacetimeModule>::DbView {
        fn connected(&self) -> u8 {
            7
        }
    }

    // (b) A foreign trait implemented for a projection type, like `impl From<AddArgs> for Reducer`.
    impl From<AddArgs> for <M as sdkstub::SpacetimeModule>::Reducer {
        fn from(a: AddArgs) -> Self {
            Self::Add { n: a.n }
        }
    }

    // (c) A callback whose parameter type is a projection, like `add_then(.., |ctx: &ReducerEventContext, ..| ..)`.
    #[allow(non_camel_case_types)]
    pub trait add {
        fn add_then(&self, n: u8, cb: impl FnOnce(&<M as sdkstub::SpacetimeModule>::ReducerEventContext, u8));
    }
    impl add for <M as sdkstub::SpacetimeModule>::DbView {
        fn add_then(&self, n: u8, cb: impl FnOnce(&<M as sdkstub::SpacetimeModule>::ReducerEventContext, u8)) {
            cb(&super::Ctx { name: "ctx" }, n)
        }
    }

    // (d) Matching the module's `Reducer` enum through an alias to a projection.
    pub fn args_from(r: &<M as sdkstub::SpacetimeModule>::Reducer) -> Option<AddArgs> {
        type R = <M as sdkstub::SpacetimeModule>::Reducer;
        #[allow(unreachable_patterns)]
        match r {
            R::Add { n } => Some(AddArgs { n: *n }),
            _ => None,
        }
    }
}

/// "Module-wide" code, as the aggregator would emit it.
pub struct RemoteModule;
pub struct RemoteTables;
pub struct Ctx {
    pub name: &'static str,
}
#[derive(Debug, PartialEq)]
pub enum Reducer {
    Add { n: u8 },
}
impl SpacetimeModule for RemoteModule {
    type DbView = RemoteTables;
    type Reducer = Reducer;
    type ReducerEventContext = Ctx;
}
impl InModule for item::AddArgs {
    type Module = RemoteModule;
}
// (e) A foreign trait implemented for a projection that normalizes to a local type:
// the aggregator only knows the table through its accessor marker.
impl InModule for <item::ConnectedTableAccessor as ClientTableDef>::Row {
    type Module = RemoteModule;
}

#[test]
fn projections_work() {
    use item::{add, ConnectedTableAccess};
    let db = RemoteTables;
    assert_eq!(db.connected(), 7);
    assert_eq!(Reducer::from(item::AddArgs { n: 3 }), Reducer::Add { n: 3 });
    db.add_then(5, |ctx, n| assert_eq!((ctx.name, n), ("ctx", 5)));
    assert_eq!(item::args_from(&Reducer::Add { n: 4 }).unwrap().n, 4);
    fn module_of<T: InModule>() -> &'static str {
        std::any::type_name::<T::Module>()
    }
    assert!(module_of::<item::Connected>().ends_with("RemoteModule"));
    assert_eq!(<item::ConnectedTableAccessor as ClientTableDef>::NAME, "connected");
}
