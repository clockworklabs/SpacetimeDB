namespace SpacetimeDB
{
    namespace Internal
    {
        [Flags]
        public enum ColumnAttrs : byte
        {
            UnSet = 0b0000,
            Indexed = 0b0001,
            AutoInc = 0b0010,
            Unique = Indexed | 0b0100,
            Identity = Unique | AutoInc,
            PrimaryKey = Unique | 0b1000,
            PrimaryKeyAuto = PrimaryKey | AutoInc,
            Default = 0b0001_0000,
        }

        [AttributeUsage(AttributeTargets.Field, AllowMultiple = true)]
        public abstract class ColumnAttribute : Attribute
        {
            public string? Table { get; init; }
            internal abstract ColumnAttrs Mask { get; }
        }
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
    /// See the <see href="https://spacetimedb.com/docs/sql">SQL reference</see> for more information.
    ///
    /// This is an experimental feature and subject to change in the future.
    /// </summary>
    [System.Diagnostics.CodeAnalysis.Experimental("STDB_UNSTABLE")]
    [AttributeUsage(AttributeTargets.Field)]
    public sealed class ClientVisibilityFilterAttribute : Attribute { }

    /// <summary>
    /// Registers a type as the row structure of a SpacetimeDB table, enabling codegen for it.
    ///
    /// <para>
    /// Multiple [Table] attributes per type are supported. This is useful to reuse row types.
    /// Each attribute instance must have a unique name and will create a SpacetimeDB table.
    /// </para>
    /// </summary>
    [AttributeUsage(AttributeTargets.Struct | AttributeTargets.Class, AllowMultiple = true)]
    public sealed class TableAttribute : Attribute
    {
        /// <summary>
        /// This identifier is used to name the SpacetimeDB table on the host as well as the
        /// table handle structures generated to access the table from within a reducer call.
        ///
        /// <para>Defaults to the <c>nameof</c> of the target type.</para>
        /// </summary>
        public string? Name { get; init; }

        /// <summary>
        /// Set to <c>true</c> to make the table visible to everyone.
        ///
        /// <para>Defaults to the table only being visible to its owner.</para>
        /// </summary>
        public bool Public { get; init; } = false;

        /// <summary>
        /// If set, the name of the reducer that will be invoked when the scheduled time is reached.
        /// </summary>
        public string? Scheduled { get; init; }

        /// <summary>
        /// The name of the column that will be used to store the scheduled time.
        ///
        /// <para>Defaults to <c>ScheduledAt</c>.</para>
        /// </summary>
        public string ScheduledAt { get; init; } = "ScheduledAt";
    }

    [AttributeUsage(
        AttributeTargets.Struct | AttributeTargets.Class | AttributeTargets.Field,
        AllowMultiple = true
    )]
    public abstract class Index : Attribute
    {
        public string? Table { get; init; }

        public string? Name { get; init; }

        public sealed class BTreeAttribute : Index
        {
            public string[] Columns { get; init; } = [];
        }
    }

    public sealed class AutoIncAttribute : Internal.ColumnAttribute
    {
        internal override Internal.ColumnAttrs Mask => Internal.ColumnAttrs.AutoInc;
    }

    public sealed class PrimaryKeyAttribute : Internal.ColumnAttribute
    {
        internal override Internal.ColumnAttrs Mask => Internal.ColumnAttrs.PrimaryKey;
    }

    public sealed class UniqueAttribute : Internal.ColumnAttribute
    {
        internal override Internal.ColumnAttrs Mask => Internal.ColumnAttrs.Unique;
    }

    /// <summary>
    /// Specifies a default value for a table column.
    /// If a column is added to an existing table while republishing of a module,
    /// the specified default value will be used to populate existing rows.
    /// </summary>
    /// <remarks>
    /// Updates existing instances of the <see cref="DefaultAttribute"/> class with the specified default value during republishing of a module.
    /// </remarks>
    /// <param name="value">The default value for the column.</param>
    [AttributeUsage(AttributeTargets.Field)]
    public sealed class DefaultAttribute(object value) : Internal.ColumnAttribute
    {
        /// <summary>
        /// The default value for the column.
        /// </summary>
        public string Value
        {
            get
            {
                if (value is null)
                {
                    return "null";
                }
                if (value is bool)
                {
                    return value.ToString()?.ToLower();
                }
                var str = value.ToString();
                if (value is string)
                {
                    str = $"\"{str}\"";
                }
                return str;
            }
        }

        internal override Internal.ColumnAttrs Mask => Internal.ColumnAttrs.Default;
    }

    public enum ReducerKind
    {
        /// <summary>
        /// The default reducer kind, no need to specify this explicitly.
        /// </summary>
        UserDefined,
        Init,
        ClientConnected,
        ClientDisconnected,
    }

    [AttributeUsage(AttributeTargets.Method, Inherited = false)]
    public sealed class ReducerAttribute(ReducerKind kind = ReducerKind.UserDefined) : Attribute
    {
        public ReducerKind Kind => kind;
    }
}
