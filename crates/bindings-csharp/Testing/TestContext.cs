namespace SpacetimeDB.Testing;

using System.Reflection;
using System.Runtime.CompilerServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

/// <summary>An isolated managed module database. Use one execution flow per fixture.</summary>
public sealed class TestContext : IDisposable
{
    private sealed record ModuleSchema(byte[] Bytes, NamespaceRegistry Namespaces);

    private static readonly ConditionalWeakTable<Assembly, ModuleSchema> schemas = [];
    private readonly DatabaseBackend backend;
    private readonly SpacetimeDB.Local db;

    private TestContext(ModuleSchema schema)
    {
        backend = new DatabaseBackend(schema.Bytes, schema.Namespaces);
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
        RawModuleDef definition = new RawModuleDef.V10(builder.BuildModuleDefinition());
        return new ModuleSchema(
            IStructuralReadWrite.ToBytes(new RawModuleDef.BSATN(), definition),
            namespaces.CreateDelegate<Func<NamespaceRegistry>>()()
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

    /// <summary>Commits on normal return; rolls back and rethrows on failure.</summary>
    public void WithReducerTx(TestAuth auth, Action<ReducerContext> body)
    {
        ValidateCallback(body, typeof(void));
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
        ValidateCallback(body, typeof(T));
        return Run(auth, body);
    }

    private T Run<T>(TestAuth auth, Func<ReducerContext, T> body)
    {
        ArgumentNullException.ThrowIfNull(auth);
        using var transaction = backend.BeginTransaction();
        var context = new ReducerContext(
            new SpacetimeDB.Local(new DatabaseAccess(transaction)),
            backend.DatabaseIdentity,
            null,
            Rng.CreateRandom(),
            Clock.Now,
            auth.SenderAuth
        );
        var result = body(context);
        // Also catch awaitables returned through object or another erased return type.
        if (result is not null && IsAwaitable(result.GetType()))
            throw new ArgumentException(
                "Reducer transaction callbacks must not return Task or ValueTask.",
                nameof(body)
            );
        transaction.Commit();
        return result;
    }

    private static void ValidateCallback(Delegate body, Type returnType)
    {
        ArgumentNullException.ThrowIfNull(body);
        if (
            body.GetInvocationList().Length != 1
            || body.Method.IsDefined(typeof(AsyncStateMachineAttribute))
            || IsAwaitable(returnType)
            || IsAwaitable(body.Method.ReturnType)
        )
            throw new ArgumentException(
                "Reducer transactions require a single synchronous callback; async void, Task and ValueTask are not supported.",
                nameof(body)
            );
    }

    private static bool IsAwaitable(Type type) =>
        typeof(Task).IsAssignableFrom(type)
        || type == typeof(ValueTask)
        || (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(ValueTask<>));

    public void Dispose() => backend.Dispose();
}
