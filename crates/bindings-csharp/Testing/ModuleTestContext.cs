namespace SpacetimeDB.Testing;

using System.Reflection;
using System.Runtime.CompilerServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

/// <summary>An isolated managed module database. Use one execution flow per fixture.</summary>
public sealed class ModuleTestContext : IDisposable
{
    private sealed record ModuleSchema(
        byte[] Bytes,
        NamespaceRegistry Namespaces,
        IReadOnlyDictionary<string, RawEnvVarTypeV10> Environment
    );

    private static readonly ConditionalWeakTable<Assembly, ModuleSchema> Schemas = [];
    private readonly DatabaseBackend _backend;

    private ModuleTestContext(ModuleSchema schema)
    {
        _backend = new DatabaseBackend(schema.Bytes, schema.Namespaces, schema.Environment);
        Db = new SpacetimeDB.Local(new DatabaseAccess(_backend));
        Clock = new TestClock(_backend);
        Rng = new TestRng(_backend);
    }

    /// <summary>Registers the specified root and its dependencies without initializing WASM.</summary>
    public static ModuleTestContext Create(Assembly module)
    {
        ArgumentNullException.ThrowIfNull(module);
        return new ModuleTestContext(Schemas.GetValue(module, BuildSchema));
    }

    private static ModuleSchema BuildSchema(Assembly module)
    {
        var descriptor =
            module.GetCustomAttribute<ModuleDescriptorAttribute>()?.DescriptorType
            ?? throw new ArgumentException(
                "The assembly is not a generated SpacetimeDB module.",
                nameof(module)
            );
        var register =
            descriptor.GetMethod("RegisterModule", [typeof(ModuleBuilder)])
            ?? throw new ArgumentException(
                "The module descriptor has no RegisterModule method.",
                nameof(module)
            );
        var namespaces =
            descriptor.GetMethod("CreateNamespaces", Type.EmptyTypes)
            ?? throw new ArgumentException(
                "The module descriptor has no CreateNamespaces method.",
                nameof(module)
            );
        var builder = new ModuleBuilder();
        register.CreateDelegate<Action<ModuleBuilder>>()(builder);
        var moduleDefinition = builder.BuildModuleDefinition();
        RawModuleDef definition = new RawModuleDef.V10(moduleDefinition);
        return new ModuleSchema(
            IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), definition),
            namespaces.CreateDelegate<Func<NamespaceRegistry>>()(),
            moduleDefinition
                .Sections.OfType<RawModuleDefV10Section.Environment>()
                .SelectMany(section => section.Environment_)
                .ToDictionary(
                    declaration => declaration.Name,
                    declaration => declaration.Ty,
                    StringComparer.Ordinal
                )
        );
    }

    public SpacetimeDB.Local Db
    {
        get
        {
            _backend.CheckActive();
            return field;
        }
    }

    public Identity DatabaseIdentity
    {
        get
        {
            _backend.CheckActive();
            return _backend.DatabaseIdentity;
        }
    }

    public TestClock Clock { get; }
    public TestRng Rng { get; }

    /// <summary>Creates a fixture-owned read-only context for the specified caller.</summary>
    public ViewContext CreateViewContext(TestAuth auth)
    {
        ArgumentNullException.ThrowIfNull(auth);
        return new ViewContext(
            auth.Sender ?? _backend.DatabaseIdentity,
            new LocalReadOnly(new DatabaseAccess(_backend))
        );
    }

    /// <summary>Creates a fixture-owned read-only context without caller identity.</summary>
    public AnonymousViewContext CreateAnonymousViewContext() =>
        new(new LocalReadOnly(new DatabaseAccess(_backend)));

    /// <summary>Creates a procedure context whose explicit transactions share this fixture.</summary>
    public ProcedureContext CreateProcedureContext(TestAuth auth) =>
        BuildProcedureContext(auth, null);

    /// <summary>Configures an HTTP responder and after-commit callback for a new procedure context.</summary>
    public ProcedureContextBuilder CreateProcedureContextBuilder(TestAuth auth)
    {
        ArgumentNullException.ThrowIfNull(auth);
        _backend.CheckActive();
        return new ProcedureContextBuilder(this, auth);
    }

    internal ProcedureContext BuildProcedureContext(
        TestAuth auth,
        Func<ModuleTestContext, HttpRequest, Result<HttpResponse, HttpError>>? responder,
        Action<ModuleTestContext>? afterCommit = null
    )
    {
        ArgumentNullException.ThrowIfNull(auth);
        _backend.CheckActive();
        return new ProcedureContext(
            _backend,
            () => Clock.Now,
            auth.Sender ?? _backend.DatabaseIdentity,
            auth.ConnectionId,
            Rng.CreateRandom(),
            auth.SenderAuth,
            new HttpClient(request => SendHttp(request, responder)),
            afterCommit is null ? null : () => afterCommit(this)
        );
    }

    private Result<HttpResponse, HttpError> SendHttp(
        HttpRequest request,
        Func<ModuleTestContext, HttpRequest, Result<HttpResponse, HttpError>>? responder
    )
    {
        _backend.CheckActive();
        return _backend.HasActiveTransaction
                ? Result<HttpResponse, HttpError>.Err(
                    new HttpError(
                        "HTTP requests cannot be performed while a mutable transaction is open (WOULD_BLOCK_TRANSACTION)."
                    )
                )
            : responder is null
                ? Result<HttpResponse, HttpError>.Err(
                    new HttpError("no test HTTP responder configured")
                )
            : responder(this, request);
    }

    /// <summary>Materializes a SELECT against committed data. Rejects active transactions.</summary>
    public IReadOnlyList<T> RunQuery<T>(IQuery<T> query)
        where T : IStructuralReadWrite, new() => _backend.RunQuery(query);

    /// <summary>Sets a declared value for this fixture, independently of database transactions.</summary>
    public void SetEnv(string key, string value) => _backend.SetEnv(key, value);

    /// <summary>Removes a declared value, including a required one for testing missing-value behavior.</summary>
    public void UnsetEnv(string key) => _backend.UnsetEnv(key);

    /// <summary>Commits on normal return; rolls back and rethrows on failure.</summary>
    public void WithReducerTx(TestAuth auth, Action<ReducerContext> body)
    {
        TransactionCallback.Validate(body, typeof(void));
        Run(
            auth,
            ctx =>
            {
                body(ctx);
                return 0;
            }
        );
    }

    /// <summary>Returns the callback value after committing. Returned error values do not roll back.</summary>
    public T WithReducerTx<T>(TestAuth auth, Func<ReducerContext, T> body)
    {
        TransactionCallback.Validate(body, typeof(T));
        return Run(auth, body);
    }

    private T Run<T>(TestAuth auth, Func<ReducerContext, T> body)
    {
        ArgumentNullException.ThrowIfNull(auth);
        using var transaction = _backend.BeginTransaction();
        var context = new ReducerContext(
            new SpacetimeDB.Local(new DatabaseAccess(transaction)),
            auth.Sender ?? _backend.DatabaseIdentity,
            auth.ConnectionId,
            Rng.CreateRandom(),
            Clock.Now,
            auth.SenderAuth
        );
        var result = body(context);
        TransactionCallback.ValidateResult(result);
        transaction.Commit();
        return result;
    }

    public void Dispose() => _backend.Dispose();
}
