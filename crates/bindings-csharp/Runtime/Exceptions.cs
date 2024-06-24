namespace SpacetimeDB;

using System;
using SpacetimeDB.Internal;

public abstract class StdbException : Exception
{
    public abstract override string Message { get; }
}

public class NoSuchTableException : StdbException
{
    public override string Message => "No such table";
}

public class LookupNotFoundException : StdbException
{
    public override string Message => "Value or range provided not found in table";
}

public class UniqueAlreadyExistsException : StdbException
{
    public override string Message => "Value with given unique identifier already exists";
}

public class BufferTooSmallException : StdbException
{
    public override string Message => "The provided buffer is not large enough to store the data";
}

public class UnknownException : StdbException
{
    private readonly FFI.Errno code;

    internal UnknownException(FFI.Errno code) => this.code = code;

    public override string Message => $"SpacetimeDB error code {code}";
}
