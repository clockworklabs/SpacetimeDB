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
        }

        [AttributeUsage(AttributeTargets.Field)]
        public abstract class ColumnAttribute : Attribute
        {
            public string? Table { get; init; }
            internal abstract ColumnAttrs Mask { get; }
        }
    }

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

        public string? Scheduled { get; init; }
    }

    namespace Index
    {
        [AttributeUsage(AttributeTargets.Struct | AttributeTargets.Class, AllowMultiple = true)]
        public sealed class BTreeAttribute : Attribute
        {
            public string? Table { get; init; }

            public string? Name { get; init; }

            public required string[] Columns { get; init; }
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

    public sealed class IndexedAttribute : Internal.ColumnAttribute
    {
        internal override Internal.ColumnAttrs Mask => Internal.ColumnAttrs.Indexed;
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
