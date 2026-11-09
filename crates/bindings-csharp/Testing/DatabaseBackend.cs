namespace SpacetimeDB.Testing;

using System.Text;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

// One execution flow owns each fixture. Native handles synchronize calls and
// destruction separately because finalizers can run on another thread.
internal sealed class DatabaseBackend : IDatabaseBackend
{
    private readonly NativeDatastore _datastore;
    private readonly IReadOnlyDictionary<string, RawEnvVarTypeV10> _environmentDeclarations;
    private readonly Dictionary<string, string> _environmentValues = new(StringComparer.Ordinal);
    private readonly Dictionary<(int Instance, string Table), uint> _tableIds = [];
    private readonly Dictionary<(int Instance, string Index), uint> _indexIds = [];

    public NamespaceRegistry Namespaces { get; }
    public Identity DatabaseIdentity => default;
    internal bool HasActiveTransaction { get; private set; }

    internal DatabaseBackend(
        ReadOnlySpan<byte> schema,
        NamespaceRegistry namespaces,
        IReadOnlyDictionary<string, RawEnvVarTypeV10>? environment = null
    )
    {
        Namespaces = namespaces;
        _datastore = NativeDatastore.Create(schema);
        _environmentDeclarations =
            environment ?? new Dictionary<string, RawEnvVarTypeV10>(StringComparer.Ordinal);
    }

    private RawEnvVarTypeV10 EnvironmentDeclaration(string key)
    {
        CheckActive();
        ArgumentNullException.ThrowIfNull(key);
        return _environmentDeclarations.TryGetValue(key, out var declaration)
            ? declaration
            : throw new ArgumentException(
                "Environment key is not declared by the root module.",
                nameof(key)
            );
    }

    public string? GetEnv(string key)
    {
        EnvironmentDeclaration(key);
        return _environmentValues.GetValueOrDefault(key);
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
        {
            throw new ArgumentException(
                "Environment value does not satisfy its declared constraint.",
                nameof(value)
            );
        }

        _environmentValues[key] = value;
    }

    internal void UnsetEnv(string key)
    {
        EnvironmentDeclaration(key);
        _environmentValues.Remove(key);
    }

    internal IReadOnlyList<T> RunQuery<T>(IQuery<T> query)
        where T : IStructuralReadWrite, new()
    {
        CheckActive();
        ArgumentNullException.ThrowIfNull(query);
        var snapshot = _datastore.Query(query.ToSql(), DatabaseIdentity);
        var offset = 0;
        var count = FramedRows.ReadCount(snapshot, ref offset);
        // Each row takes at least its 4-byte length, so this bounds the allocation.
        if (count > (uint)(snapshot.Length - offset) / sizeof(uint))
        {
            throw new InvalidDataException("Truncated native row snapshot.");
        }

        var rows = new T[count];
        for (var i = 0; i < rows.Length; i++)
        {
            rows[i] = FramedRows.ReadRow<T>(snapshot, ref offset);
        }

        FramedRows.CheckEnd(snapshot, offset);
        return rows;
    }

    public void CheckActive() => ObjectDisposedException.ThrowIf(_datastore.IsClosed, this);

    public IDatabaseTransaction BeginTransaction()
    {
        CheckActive();
        var transaction = new Transaction(this, _datastore.Begin());
        HasActiveTransaction = true;
        return transaction;
    }

    public void Dispose() => _datastore.Dispose();

    private sealed class Transaction(DatabaseBackend backend, NativeTransaction transaction)
        : IDatabaseTransaction
    {
        private bool finished;

        public IDatabaseBackend Backend => backend;

        private uint TableId(int instanceId, string table)
        {
            var key = (instanceId, table);
            if (!backend._tableIds.TryGetValue(key, out var id))
            {
                id = transaction.TableId(backend.Namespaces.Resolve(instanceId, table));
                backend._tableIds.Add(key, id);
            }
            return id;
        }

        private uint IndexId(int instanceId, string index)
        {
            var key = (instanceId, index);
            if (!backend._indexIds.TryGetValue(key, out var id))
            {
                id = transaction.IndexId(backend.Namespaces.Resolve(instanceId, index));
                backend._indexIds.Add(key, id);
            }
            return id;
        }

        public byte[] IndexScanPoint(int instanceId, string index, byte[] point)
        {
            CheckActive();
            return transaction.IndexScanPoint(IndexId(instanceId, index), point);
        }

        public bool IndexDeletePoint(int instanceId, string index, byte[] point)
        {
            CheckActive();
            return transaction.IndexDeletePoint(IndexId(instanceId, index), point);
        }

        public byte[] IndexScanRange(
            int instanceId,
            string index,
            ushort prefixElems,
            byte[] prefix,
            byte[] start,
            byte[] end
        )
        {
            CheckActive();
            return transaction.IndexScanRange(
                IndexId(instanceId, index),
                prefixElems,
                prefix,
                start,
                end
            );
        }

        public uint IndexDeleteRange(
            int instanceId,
            string index,
            ushort prefixElems,
            byte[] prefix,
            byte[] start,
            byte[] end
        )
        {
            CheckActive();
            return transaction.IndexDeleteRange(
                IndexId(instanceId, index),
                prefixElems,
                prefix,
                start,
                end
            );
        }

        public byte[] Update(int instanceId, string table, string index, byte[] row)
        {
            CheckActive();
            return transaction.Update(TableId(instanceId, table), IndexId(instanceId, index), row);
        }

        public ulong Count(int instanceId, string table)
        {
            CheckActive();
            return transaction.Count(TableId(instanceId, table));
        }

        public byte[] Insert(int instanceId, string table, byte[] row)
        {
            CheckActive();
            return transaction.Insert(TableId(instanceId, table), row);
        }

        public bool Delete(int instanceId, string table, byte[] row)
        {
            CheckActive();
            return transaction.Delete(TableId(instanceId, table), row);
        }

        public ulong Clear(int instanceId, string table)
        {
            CheckActive();
            return transaction.Clear(TableId(instanceId, table));
        }

        public byte[] Scan(int instanceId, string table)
        {
            CheckActive();
            return transaction.Scan(TableId(instanceId, table));
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
                Finish();
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
                Finish();
            }
        }

        private void Finish()
        {
            if (finished)
            {
                return;
            }

            finished = true;
            backend.HasActiveTransaction = false;
        }

        public void Dispose()
        {
            try
            {
                transaction.Dispose();
            }
            finally
            {
                Finish();
            }
        }
    }
}
