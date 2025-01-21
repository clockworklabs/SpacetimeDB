namespace SpacetimeDB.Internal;

using System.Diagnostics;
using System.Text;

// SpacetimeDB modules are guaranteed to be single-threaded, and, unlike in iterators, we don't have any potential for single-threaded concurrency.
// This means we can use a single static buffer for all serialization operations, as long as we don't use it concurrently (e.g. in iterators).
// In order to ensure that, this struct acts as a singleton guard to the data that actually lives in a static scope.
public class SerializationBuffer : MemoryStream
{
    public readonly BinaryReader Reader;
    public readonly BinaryWriter Writer;
    private bool isUsed = false;

    private SerializationBuffer()
    {
        Reader = new(this, Encoding.UTF8, leaveOpen: true);
        Writer = new(this, Encoding.UTF8, leaveOpen: true);
    }

    private static readonly SerializationBuffer instance = new();

    public static SerializationBuffer Borrow()
    {
        Debug.Assert(!instance.isUsed, "Buffer is already in use");
        instance.isUsed = true;
        instance.Position = 0;
        return instance;
    }

    public Span<byte> GetWritten() => GetBuffer().AsSpan(0, (int)Position);

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            isUsed = false;
        }
    }
}
