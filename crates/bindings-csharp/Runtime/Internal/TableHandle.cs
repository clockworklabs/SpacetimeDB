namespace SpacetimeDB.Internal;

/// <summary>A table ID cached for one published table instance.</summary>
public abstract class TableHandle(string name)
{
    private string? lookupName = name;
    private FFI.TableId? id;

    internal FFI.TableId Id => id ??= ResolveId();

    private FFI.TableId ResolveId()
    {
        // Module execution is single-threaded. Keep lookup lazy without a Lazy/delegate allocation.
        var bytes = System.Text.Encoding.UTF8.GetBytes(lookupName!);
        FFI.table_id_from_name(bytes, bytes.Length, out var result);
        lookupName = null;
        return result;
    }
}
