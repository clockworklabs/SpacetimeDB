use spacetimedb_sdk::{
    anyhow::Result,
    callbacks::CallbackId,
    spacetime_module::InModule,
    spacetimedb_lib::{de::Deserialize, ser::Serialize},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetName {
    pub text: String,
}

impl InModule for SetName {
    type Module = super::RemoteModule;
}

pub struct SetNameCallbackId(CallbackId);

#[allow(non_camel_case_types)]
pub trait set_name {
    fn set_name(&self, text: String) -> Result<()>;
    fn on_set_name(
        &self,
        callback: impl FnMut(&super::EventContext, &String) + Send + Sync + 'static,
    ) -> SetNameCallbackId;
    fn remove_on_set_name(&self, callback: SetNameCallbackId);
}

impl set_name for super::RemoteReducers {
    fn set_name(&self, text: String) -> Result<()> {
        self.imp.call_reducer("set_name", SetName { text })
    }
    fn on_set_name(
        &self,
        mut callback: impl FnMut(&super::EventContext, &String) + Send + Sync + 'static,
    ) -> SetNameCallbackId {
        SetNameCallbackId(self.imp.on_reducer::<SetName>(
            "set_name",
            Box::new(move |ctx: &super::EventContext, args: &SetName| callback(ctx, &args.text)),
        ))
    }
    fn remove_on_set_name(&self, callback: SetNameCallbackId) {
        self.imp.remove_on_reducer::<SetName>("set_name", callback.0)
    }
}
