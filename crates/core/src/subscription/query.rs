use super::execution_unit::QueryHash;
use super::module_subscription_manager::Plan;
use crate::db::relational_db::Tx;
use crate::db::sql::ast::SchemaViewer;
use crate::error::{DBError, SubscriptionError};
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_execution::{Datastore, ExecutionParams};
use spacetimedb_expr::check::SchemaView;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::ProductValue;
use spacetimedb_physical_plan::plan::{ParamResolver, ParamSource, ProjectPlan};
use spacetimedb_query::compile_subscription_template;
use spacetimedb_subscription::{SubscriptionPlan, Template};
use std::sync::Arc;

static WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*$").unwrap());
static SUBSCRIBE_TO_ALL_TABLES_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?i)\bSELECT\s+\*\s+FROM\s+\*\s*$").unwrap());

/// Is this string all whitespace?
pub fn is_whitespace_or_empty(sql: &str) -> bool {
    WHITESPACE.is_match_at(sql, 0)
}

/// Is this a `SELECT * FROM *` query?
pub fn is_subscribe_to_all_tables(sql: &str) -> bool {
    SUBSCRIBE_TO_ALL_TABLES_REGEX.is_match_at(sql, 0)
}

pub(crate) struct CompiledQuery {
    pub plan: Plan,
    /// Optimized physical plans used only for row-limit estimation on newly compiled queries.
    pub physical_plans: Vec<ProjectPlan>,
}

/// Compile a string into a single read-only query.
pub fn compile_read_only_query(auth: &AuthCtx, tx: &Tx, input: &str) -> Result<Plan, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let tx = SchemaViewer::new(tx, auth);
    let (plans, has_param, _) = SubscriptionPlan::compile_plans(input, &tx, auth)?;
    let (template, values) = compile_template(input, &tx, auth, &mut TemplateCache::default())?;
    let hash = QueryHash::from_string(input, auth.caller(), has_param);
    Ok(Plan::new(plans, template, values, hash, input.to_owned()))
}

/// Compile a string into a single read-only query with externally-computed hashes.
/// It shares the template of a query in `templates` if one compiles to the same.
pub(crate) fn compile_query_with_hashes<Tx: Datastore + StateView>(
    auth: &AuthCtx,
    tx: &Tx,
    input: &str,
    hash: QueryHash,
    hash_with_param: QueryHash,
    templates: &mut TemplateCache,
) -> Result<CompiledQuery, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let tx = SchemaViewer::new(tx, auth);
    let (plans, has_param, physical_plans) = SubscriptionPlan::compile_plans(input, &tx, auth)?;
    let (template, values) = compile_template(input, &tx, auth, templates)?;

    let hash = if auth.bypass_rls() || has_param {
        hash_with_param
    } else {
        hash
    };
    Ok(CompiledQuery {
        plan: Plan::new(plans, template, values, hash, input.to_owned()),
        physical_plans,
    })
}

/// The templates a compiled query shares: those of subscribed queries, and those compiled since,
/// so the queries of one subscribe share a template before any of them is subscribed
#[derive(Default)]
pub(crate) struct TemplateCache<'a> {
    subscribed: Option<&'a FindTemplate<'a>>,
    /// The templates compiled since, by the hash of their key
    compiled: IntMap<u64, Vec<Arc<Template>>>,
}

/// Finds the template of a subscribed query by the hash of its key and its plans,
/// such as with [`TemplateStore::find`](super::module_subscription_manager::TemplateStore::find)
pub(crate) type FindTemplate<'a> = dyn Fn(u64, &[ProjectPlan]) -> Option<Arc<Template>> + 'a;

impl<'a> TemplateCache<'a> {
    pub fn new(subscribed: &'a FindTemplate<'a>) -> Self {
        Self {
            subscribed: Some(subscribed),
            compiled: IntMap::default(),
        }
    }

    /// The template that `plans` compile to, if a query here uses it
    fn find(&self, plans: &[ProjectPlan]) -> Option<Arc<Template>> {
        let key_hash = Template::hash_key(plans);
        let subscribed = self.subscribed.and_then(|find| find(key_hash, plans));
        subscribed.or_else(|| {
            let compiled = self.compiled.get(&key_hash)?;
            compiled.iter().find(|template| template.key() == plans).cloned()
        })
    }

    /// Adds a newly compiled template, for later queries to share
    fn insert(&mut self, template: Template) -> Arc<Template> {
        let template = Arc::new(template);
        self.compiled
            .entry(template.key_hash())
            .or_default()
            .push(template.clone());
        template
    }
}

/// Compiles a subscription as a template, which runs its deltas for every query that differs only in its values.
/// Returns the template, shared with `templates` if a query there uses it, and this query's values for its params.
pub(crate) fn compile_template(
    input: &str,
    tx: &impl SchemaView,
    auth: &AuthCtx,
    templates: &mut TemplateCache,
) -> Result<(Arc<Template>, ProductValue), DBError> {
    let compiled = compile_subscription_template(input, tx, auth)?;
    // Only a new template builds its delta plans
    let template = match templates.find(&compiled.plans) {
        Some(template) => template,
        None => templates.insert(Template::build(&compiled)?),
    };
    let builtins = ExecutionParams::from_auth(auth);
    let values = compiled
        .params
        .iter()
        .map(|(source, ty)| match source {
            ParamSource::Literal(value) => value.clone(),
            ParamSource::Builtin(slot) => builtins.resolve_param(*slot, ty),
        })
        .collect();
    Ok((template, values))
}
