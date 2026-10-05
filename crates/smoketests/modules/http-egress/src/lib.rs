use spacetimedb::ProcedureContext;

#[spacetimedb::procedure]
pub fn request_disallowed_ip(ctx: &mut ProcedureContext, url: String) -> Result<(), String> {
    match ctx.http.get(url) {
        Ok(_) => Err("request unexpectedly succeeded".to_owned()),
        Err(err) => {
            let message = err.to_string();
            if message.contains("refusing to connect to private or special-purpose addresses") {
                Ok(())
            } else {
                Err(format!("unexpected error from http request: {message}"))
            }
        }
    }
}
