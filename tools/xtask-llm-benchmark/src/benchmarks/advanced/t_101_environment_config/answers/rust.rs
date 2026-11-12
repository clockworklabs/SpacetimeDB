#![allow(non_snake_case)]
use spacetimedb::{AnonymousViewContext, ReducerContext, SpacetimeType, Table};

#[spacetimedb::env]
pub struct Env {
    pub REGION: String,
    #[env(values("development", "production"))]
    pub MODE: String,
    pub BANNER: Option<String>,
}

#[spacetimedb::table(accessor = deployment_note, public)]
pub struct DeploymentNote {
    #[primary_key]
    pub id: u32,
    pub region: String,
    pub mode: String,
    pub banner: String,
}

#[derive(SpacetimeType)]
pub struct DeploymentInfo {
    pub region: String,
    pub mode: String,
}

#[spacetimedb::reducer]
pub fn record_deployment(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    if ctx.db.deployment_note().id().find(id).is_some() {
        return Err("note exists".to_string());
    }
    ctx.db.deployment_note().insert(DeploymentNote {
        id,
        region: ctx.env.REGION(),
        mode: ctx.env.MODE(),
        banner: ctx.env.BANNER().unwrap_or_else(|| "(none)".to_string()),
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn require_development(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.env.MODE() == "production" {
        return Err("production locked".to_string());
    }
    Ok(())
}

#[spacetimedb::view(accessor = current_deployment, public)]
pub fn current_deployment(ctx: &AnonymousViewContext) -> Vec<DeploymentInfo> {
    vec![DeploymentInfo {
        region: ctx.env.REGION(),
        mode: ctx.env.MODE(),
    }]
}
