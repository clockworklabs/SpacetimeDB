namespace SpacetimeDB
{
    // Client bindings declare reducers, procedures, and views the way a module does,
    // with a context as the first parameter. Clients don't run these functions,
    // so on the client the context types only exist for the declarations to compile.

    /// <summary>
    /// The context of a reducer declaration in client bindings.
    /// To call a reducer, use <c>conn.Reducers</c>.
    /// </summary>
    public sealed class ReducerContext
    {
        private ReducerContext() { }
    }

    /// <summary>
    /// The context of a procedure declaration in client bindings.
    /// To call a procedure, use <c>conn.Procedures</c>.
    /// </summary>
    public sealed class ProcedureContext
    {
        private ProcedureContext() { }
    }

    /// <summary>
    /// The context of a view declaration in client bindings.
    /// To read a view, subscribe to it and use <c>conn.Db</c>.
    /// </summary>
    public sealed class ViewContext
    {
        private ViewContext() { }
    }

    /// <summary>
    /// The context of an anonymous view declaration in client bindings.
    /// To read a view, subscribe to it and use <c>conn.Db</c>.
    /// </summary>
    public sealed class AnonymousViewContext
    {
        private AnonymousViewContext() { }
    }
}
