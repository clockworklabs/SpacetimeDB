namespace SpacetimeDB;

// Attributes that client bindings also use, such as [Table] and [Reducer], live in
// BSATN.Runtime/ModuleAttrs.cs. The ones here only apply to modules.

/// <summary>Declares the complete environment schema for this module.</summary>
[AttributeUsage(AttributeTargets.Struct)]
public sealed class EnvAttribute : Attribute { }

/// <summary>Restricts one declared string to these exact permitted values.</summary>
[AttributeUsage(AttributeTargets.Field)]
public sealed class EnvValuesAttribute(params string[] values) : Attribute
{
    public string[] Values { get; } = values;
}

/// <summary>
/// Generates code for registering a row-level security rule.
///
/// This attribute must be applied to a <c>static</c> field of type <c>Filter</c>.
/// It will be interpreted as a filter on the table to which it applies, for all client queries.
/// If a module contains multiple <c>client_visibility_filter</c>s for the same table,
/// they will be unioned together as if by SQL <c>OR</c>,
/// so that any row permitted by at least one filter is visible.
///
/// The query follows the same syntax as a subscription query.
/// See the <see href="https://spacetimedb.com/docs/reference/sql">SQL reference</see> for more information.
///
/// This is an experimental feature and subject to change in the future.
/// </summary>
[System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
[AttributeUsage(AttributeTargets.Field)]
public sealed class ClientVisibilityFilterAttribute : Attribute { }

[AttributeUsage(AttributeTargets.Field)]
public sealed class SettingsAttribute : Attribute { }

[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class HttpHandlerAttribute() : Attribute { }

[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class HttpRouterAttribute() : Attribute { }
