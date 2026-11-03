namespace SpacetimeDB.Testing;

using System.Reflection;
using System.Runtime.CompilerServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

/// <summary>An isolated managed module database. Use one execution flow per fixture.</summary>
public sealed class TestContext : IDisposable
{
    private sealed record ModuleSchema(
        byte[] Bytes,
        NamespaceRegistry Namespaces,
        IReadOnlyDictionary<string, RawEnvVarTypeV10> Environment
    );

    private static readonly ConditionalWeakTable<Assembly, ModuleSchema> schemas = [];
    private readonly DatabaseBackend backend;
    private readonly SpacetimeDB.Local db;

    private TestContext(ModuleSchema schema)
    {
        backend = new DatabaseBackend(schema.Bytes, schema.Namespaces, schema.Environment);
        db = new SpacetimeDB.Local(new DatabaseAccess(backend));
        Clock = new TestClock(backend);
        Rng = new TestRng(backend);
    }

    /// <summary>Registers the specified root and its dependencies without initializing WASM.</summary>
    public static TestContext Create(Assembly module)
    {
        ArgumentNullException.ThrowIfNull(module);
        return new TestContext(schemas.GetValue(module, BuildSchema));
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
            backend.CheckActive();
            return db;
        }
    }

    public Identity DatabaseIdentity
    {
        get
        {
            backend.CheckActive();
            return backend.DatabaseIdentity;
        }
    }

    public TestClock Clock { get; }
    public TestRng Rng { get; }

    /// <summary>Creates a fixture-owned read-only context for the specified caller.</summary>
    public ViewContext ViewContext(TestAuth auth)
    {
        ArgumentNullException.ThrowIfNull(auth);
        return new ViewContext(
            auth.Sender ?? backend.DatabaseIdentity,
            new LocalReadOnly(new DatabaseAccess(backend))
        );
    }

    /// <summary>Creates a fixture-owned read-only context without caller identity.</summary>
    public AnonymousViewContext AnonymousViewContext() =>
        new(new LocalReadOnly(new DatabaseAccess(backend)));

    /// <summary>Creates a procedure context whose explicit transactions share this fixture.</summary>
    public ProcedureContext ProcedureContext(TestAuth auth) => CreateProcedureContext(auth, null);

    /// <summary>Configures responders for a new procedure context.</summary>
    public ProcedureContextBuilder ProcedureContextBuilder(TestAuth auth)
    {
        ArgumentNullException.ThrowIfNull(auth);
        backend.CheckActive();
        return new(this, auth);
    }

    internal ProcedureContext CreateProcedureContext(
        TestAuth auth,
        Func<TestContext, HttpRequest, Result<HttpResponse, HttpError>>? responder
    )
    {
        ArgumentNullException.ThrowIfNull(auth);
        backend.CheckActive();
        return new ProcedureContext(
            backend,
            () => Clock.Now,
            auth.Sender ?? backend.DatabaseIdentity,
            auth.ConnectionId,
            Rng.CreateRandom(),
            auth.SenderAuth,
            new HttpClient(request => SendHttp(request, responder))
        );
    }

    private Result<HttpResponse, HttpError> SendHttp(
        HttpRequest request,
        Func<TestContext, HttpRequest, Result<HttpResponse, HttpError>>? responder
    )
    {
        backend.CheckActive();
        if (backend.HasActiveTransaction)
            return Result<HttpResponse, HttpError>.Err(
                new HttpError(
                    "HTTP requests cannot be performed while a mutable transaction is open (WOULD_BLOCK_TRANSACTION)."
                )
            );
        if (responder is null)
            return Result<HttpResponse, HttpError>.Err(
                new HttpError("no test HTTP responder configured")
            );
        ArgumentNullException.ThrowIfNull(request);
        if (string.IsNullOrEmpty(request.Uri))
            return Result<HttpResponse, HttpError>.Err(
                new HttpError("URI must not be null or empty")
            );
        if (request.Timeout < TimeSpan.Zero)
            return Result<HttpResponse, HttpError>.Err(
                new HttpError("Timeout must not be negative")
            );
        if (request.Timeout > TimeSpan.FromSeconds(180))
            request = new HttpRequest
            {
                Uri = request.Uri,
                Method = request.Method,
                Headers = request.Headers,
                Body = request.Body,
                Version = request.Version,
                Timeout = TimeSpan.FromSeconds(180),
            };
        return responder(this, request);
    }

    /// <summary>Materializes a SELECT against committed data. Rejects active transactions.</summary>
    public IReadOnlyList<T> RunQuery<T>(IQuery<T> query)
        where T : IStructuralReadWrite, new() => backend.RunQuery(query);

    /// <summary>Sets a declared value for this fixture, independently of database transactions.</summary>
    public void SetEnv(string key, string value) => backend.SetEnv(key, value);

    /// <summary>Removes a declared value, including a required one for testing missing-value behavior.</summary>
    public void UnsetEnv(string key) => backend.UnsetEnv(key);

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
        using var transaction = backend.BeginTransaction();
        var context = new ReducerContext(
            new SpacetimeDB.Local(new DatabaseAccess(transaction)),
            auth.Sender ?? backend.DatabaseIdentity,
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

    public void Dispose() => backend.Dispose();
}
