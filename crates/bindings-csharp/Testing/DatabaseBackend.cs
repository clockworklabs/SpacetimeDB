namespace SpacetimeDB.Testing;

using System.Text;
using SpacetimeDB.Internal;

// One execution flow owns each fixture. Native handles synchronize calls and
// destruction separately because finalizers can run on another thread.
internal sealed class DatabaseBackend : IDatabaseBackend
{
    private readonly NativeDatastore datastore;
    private readonly IReadOnlyDictionary<string, RawEnvVarTypeV10> environmentDeclarations;
    private readonly Dictionary<string, string> environmentValues = new(StringComparer.Ordinal);
    private readonly Dictionary<(string Assembly, string Table), uint> tableIds = [];
    private readonly Dictionary<(string Assembly, string Index), uint> indexIds = [];

    public NamespaceRegistry Namespaces { get; }
    public Identity DatabaseIdentity => default;

    internal DatabaseBackend(
        ReadOnlySpan<byte> schema,
        NamespaceRegistry namespaces,
        IReadOnlyDictionary<string, RawEnvVarTypeV10>? environment = null
    )
    {
        Namespaces = namespaces;
        datastore = NativeDatastore.Create(schema);
        environmentDeclarations =
            environment ?? new Dictionary<string, RawEnvVarTypeV10>(StringComparer.Ordinal);
    }

    private RawEnvVarTypeV10 EnvironmentDeclaration(string key)
    {
        CheckActive();
        ArgumentNullException.ThrowIfNull(key);
        return environmentDeclarations.TryGetValue(key, out var declaration)
            ? declaration
            : throw new ArgumentException(
                "Environment key is not declared by the root module.",
                nameof(key)
            );
    }

    public string? GetEnv(string key)
    {
        EnvironmentDeclaration(key);
        return environmentValues.GetValueOrDefault(key);
    }

    internal void SetEnv(string key, string value)
    {
        var declaration = EnvironmentDeclaration(key);
        ArgumentNullException.ThrowIfNull(value);
        if (Encoding.UTF8.GetByteCount(value) > 8192)
            throw new ArgumentException(
                "Environment values must not exceed 8192 UTF-8 bytes.",
                nameof(value)
            );
        var allowed = declaration switch
        {
            RawEnvVarTypeV10.String => true,
            RawEnvVarTypeV10.StringLiteral(var literal) => value == literal,
            RawEnvVarTypeV10.Union(var literals) => literals.Contains(value),
            _ => throw new InvalidOperationException("Unknown environment declaration type."),
        };
        if (!allowed)
            throw new ArgumentException(
                "Environment value does not satisfy its declared constraint.",
                nameof(value)
            );
        environmentValues[key] = value;
    }

    internal void UnsetEnv(string key)
    {
        EnvironmentDeclaration(key);
        environmentValues.Remove(key);
    }

    public void CheckActive() => ObjectDisposedException.ThrowIf(datastore.IsClosed, this);

    public IDatabaseTransaction BeginTransaction()
    {
        CheckActive();
        return new Transaction(this, datastore.Begin());
    }

    public void Dispose() => datastore.Dispose();

    private sealed class Transaction(DatabaseBackend backend, NativeTransaction transaction)
        : IDatabaseTransaction
    {
        private bool finished;

        public IDatabaseBackend Backend => backend;

        private uint TableId(string assemblyIdentity, string table)
        {
            var key = (assemblyIdentity, table);
            if (!backend.tableIds.TryGetValue(key, out var id))
            {
                id = transaction.TableId(backend.Namespaces.Resolve(assemblyIdentity, table));
                backend.tableIds.Add(key, id);
            }
            return id;
        }

        private uint IndexId(string assemblyIdentity, string index)
        {
            var key = (assemblyIdentity, index);
            if (!backend.indexIds.TryGetValue(key, out var id))
            {
                id = transaction.IndexId(backend.Namespaces.Resolve(assemblyIdentity, index));
                backend.indexIds.Add(key, id);
            }
            return id;
        }

        public byte[] IndexScanPoint(string assemblyIdentity, string index, byte[] point)
        {
            CheckActive();
            return transaction.IndexScanPoint(IndexId(assemblyIdentity, index), point);
        }

        public bool IndexDeletePoint(string assemblyIdentity, string index, byte[] point)
        {
            CheckActive();
            return transaction.IndexDeletePoint(IndexId(assemblyIdentity, index), point);
        }

        public byte[] IndexScanRange(
            string assemblyIdentity,
            string index,
            ushort prefixElems,
            byte[] prefix,
            byte[] start,
            byte[] end
        )
        {
            CheckActive();
            return transaction.IndexScanRange(
                IndexId(assemblyIdentity, index),
                prefixElems,
                prefix,
                start,
                end
            );
        }

        public uint IndexDeleteRange(
            string assemblyIdentity,
            string index,
            ushort prefixElems,
            byte[] prefix,
            byte[] start,
            byte[] end
        )
        {
            CheckActive();
            return transaction.IndexDeleteRange(
                IndexId(assemblyIdentity, index),
                prefixElems,
                prefix,
                start,
                end
            );
        }

        public byte[] Update(string assemblyIdentity, string table, string index, byte[] row)
        {
            CheckActive();
            return transaction.Update(
                TableId(assemblyIdentity, table),
                IndexId(assemblyIdentity, index),
                row
            );
        }

        public ulong Count(string assemblyIdentity, string table)
        {
            CheckActive();
            return transaction.Count(TableId(assemblyIdentity, table));
        }

        public byte[] Insert(string assemblyIdentity, string table, byte[] row)
        {
            CheckActive();
            return transaction.Insert(TableId(assemblyIdentity, table), row);
        }

        public bool Delete(string assemblyIdentity, string table, byte[] row)
        {
            CheckActive();
            return transaction.Delete(TableId(assemblyIdentity, table), row);
        }

        public ulong Clear(string assemblyIdentity, string table)
        {
            CheckActive();
            return transaction.Clear(TableId(assemblyIdentity, table));
        }

        public byte[] Scan(string assemblyIdentity, string table)
        {
            CheckActive();
            return transaction.Scan(TableId(assemblyIdentity, table));
        }

        public void CheckActive()
        {
            backend.CheckActive();
            ObjectDisposedException.ThrowIf(transaction.IsClosed, this);
            if (finished)
                throw new InvalidOperationException("The transaction has finished.");
        }

        public void Commit()
        {
            CheckActive();
            try
            {
                transaction.Commit();
            }
            finally
            {
                finished = true;
            }
        }

        public void Rollback()
        {
            CheckActive();
            try
            {
                transaction.Rollback();
            }
            finally
            {
                finished = true;
            }
        }

        public void Dispose() => transaction.Dispose();
    }
}
