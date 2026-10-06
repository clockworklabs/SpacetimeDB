namespace SpacetimeDB.Internal;

/// <summary>A table ID cached for one published table instance.</summary>
public abstract class TableHandle(string name)
{
    private readonly Lazy<FFI.TableId> id =
        new(() =>
        {
            var bytes = System.Text.Encoding.UTF8.GetBytes(name);
            FFI.table_id_from_name(bytes, bytes.Length, out var result);
            return result;
        });

    internal FFI.TableId Id => id.Value;
}
