use spacetimedb_sdk::{
    anyhow::Result,
    callbacks::CallbackId,
    spacetime_module::InModule,
    spacetimedb_lib::{de::Deserialize, ser::Serialize},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendMessage {
    pub text: String,
}

impl InModule for SendMessage {
    type Module = super::RemoteModule;
}

pub struct SendMessageCallbackId(CallbackId);

#[allow(non_camel_case_types)]
pub trait send_message {
    fn send_message(&self, text: String) -> Result<()>;
    fn on_send_message(
        &self,
        callback: impl FnMut(&super::EventContext, &String) + Send + Sync + 'static,
    ) -> SendMessageCallbackId;
    fn remove_on_send_message(&self, callback: SendMessageCallbackId);
}

impl send_message for super::RemoteReducers {
    fn send_message(&self, text: String) -> Result<()> {
        self.imp.call_reducer("send_message", SendMessage { text })
    }
    fn on_send_message(
        &self,
        mut callback: impl FnMut(&super::EventContext, &String) + Send + Sync + 'static,
    ) -> SendMessageCallbackId {
        SendMessageCallbackId(self.imp.on_reducer::<SendMessage>(
            "send_message",
            Box::new(move |ctx: &super::EventContext, args: &SendMessage| callback(ctx, &args.text)),
        ))
    }
    fn remove_on_send_message(&self, callback: SendMessageCallbackId) {
        self.imp.remove_on_reducer::<SendMessage>("send_message", callback.0)
    }
}
