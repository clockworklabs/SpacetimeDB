namespace SpacetimeDB;

using SpacetimeDB.Internal;

public abstract class StdbException : Exception
{
    public abstract override string Message { get; }
}

public class NotInTransactionException : StdbException
{
    public override string Message => "ABI call can only be made while in a transaction";
}

public class BsatnDecodeException : StdbException
{
    public override string Message => "Couldn't decode the BSATN to the expected type";
}

public class NoSuchTableException : StdbException
{
    public override string Message => "No such table";
}

public class NoSuchIndexException : StdbException
{
    public override string Message => "No such index";
}

public class IndexNotUniqueException : StdbException
{
    public override string Message => "The index was not unique";
}

public class NoSuchRowException : StdbException
{
    public override string Message => "The row was not found, e.g., in an update call";
}

public class UniqueConstraintViolationException : StdbException
{
    public override string Message => "Value with given unique identifier already exists";
}

public class ScheduleAtDelayTooLongException : StdbException
{
    public override string Message => "Specified delay in scheduling row was too long";
}

public class BufferTooSmallException : StdbException
{
    public override string Message => "The provided buffer is not large enough to store the data";
}

public class NoSuchIterException : StdbException
{
    public override string Message => "The provided row iterator does not exist";
}

public class NoSuchLogStopwatch : StdbException
{
    public override string Message => "The provided stopwatch does not exist";
}

public class NoSuchBytesException : StdbException
{
    public override string Message => "The provided bytes source or sink does not exist";
}

public class NoSpaceException : StdbException
{
    public override string Message => "The provided bytes sink has no more room left";
}

public class AutoIncOverflowException : StdbException
{
    public override string Message => "The auto-increment sequence overflowed";
}

public class UnknownException : StdbException
{
    private readonly Errno code;

    internal UnknownException(Errno code) => this.code = code;

    public override string Message => $"SpacetimeDB error code {code}";
}
