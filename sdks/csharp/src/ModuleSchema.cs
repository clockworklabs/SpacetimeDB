using System;

namespace SpacetimeDB
{
    /// <summary>Selects the schema directory for this generated client connection, relative to its source file.</summary>
    [AttributeUsage(AttributeTargets.Class, Inherited = false)]
    public sealed class ClientConnectionAttribute : Attribute
    {
        public string SchemaPath { get; }

        public ClientConnectionAttribute(string schemaPath)
        {
            SchemaPath = schemaPath;
        }
    }

    /// <summary>Describes an index or unique constraint with no equivalent C# module attribute.</summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, AllowMultiple = true)]
    public sealed class ClientIndexAttribute : Attribute
    {
        public string[] Columns { get; }
        public string? Accessor { get; }
        public bool Unique { get; }
        public string? Table { get; set; }

        public ClientIndexAttribute(string[] columns, string? accessor = null, bool unique = false)
        {
            Columns = columns;
            Accessor = accessor;
            Unique = unique;
        }
    }

    // These types let a client describe server signatures. Only the server supplies a context.
    public sealed class ReducerContext
    {
        private ReducerContext() { }
    }

    public sealed class ProcedureContext
    {
        private ProcedureContext() { }
    }

    public sealed class ViewContext
    {
        private ViewContext() { }
    }

    public sealed class AnonymousViewContext
    {
        private AnonymousViewContext() { }
    }
}
