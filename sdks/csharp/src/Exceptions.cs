using System;
using System.Runtime.Serialization;

namespace SpacetimeDB
{
    /// <summary>
    /// The base class for all SpacetimeDB SDK exceptions.
    /// This allows users to catch all SpacetimeDB-specific exceptions in one catch block
    /// or configure their debugger to ignore them.
    /// </summary>
    [Serializable]
    public class SpacetimeDBException : Exception
    {
        // Base HRESULT for SpacetimeDB exceptions (0x8A000000 is in the user-defined range)
        private const int SPACETIMEDB_HRESULT_BASE = unchecked((int)0x8A000000);

        // Specific HRESULTs for different exception types
        public const int SPACETIMEDB_EMPTY_REDUCER_NAME = SPACETIMEDB_HRESULT_BASE + 1;

        public SpacetimeDBException()
            : base()
        {
            HResult = SPACETIMEDB_HRESULT_BASE;
        }

        public SpacetimeDBException(string? message)
            : base(message)
        {
            HResult = SPACETIMEDB_HRESULT_BASE;
        }

        public SpacetimeDBException(string? message, Exception? innerException)
            : base(message, innerException)
        {
            HResult = SPACETIMEDB_HRESULT_BASE;
        }

        protected SpacetimeDBException(SerializationInfo info, StreamingContext context)
            : base(info, context) { }
    }

    /// <summary>
    /// The exception that is thrown when one of the arguments provided to a method is not valid.
    /// This is the base class for all SpacetimeDB argument exceptions.
    /// </summary>
    [Serializable]
    public class SpacetimeDBArgumentException : SpacetimeDBException
    {
        private readonly string? _paramName;

        public SpacetimeDBArgumentException()
            : base("Value does not fall within the expected range.") { }

        public SpacetimeDBArgumentException(string? message)
            : base(message) { }

        public SpacetimeDBArgumentException(string? message, Exception? innerException)
            : base(message, innerException) { }

        public SpacetimeDBArgumentException(
            string? message,
            string? paramName,
            Exception? innerException
        )
            : base(message, innerException)
        {
            _paramName = paramName;
        }

        public SpacetimeDBArgumentException(string? message, string? paramName)
            : base(message)
        {
            _paramName = paramName;
        }

        protected SpacetimeDBArgumentException(SerializationInfo info, StreamingContext context)
            : base(info, context)
        {
            _paramName = info.GetString("ParamName");
        }

        public override void GetObjectData(SerializationInfo info, StreamingContext context)
        {
            base.GetObjectData(info, context);
            info.AddValue("ParamName", _paramName, typeof(string));
        }

        public virtual string? ParamName => _paramName;
    }

    /// <summary>
    /// The exception that is thrown when an empty reducer name is received from the server.
    /// This is a known condition that is handled internally by the SpacetimeDB client.
    /// </summary>
    [Serializable]
    public class SpacetimeDBEmptyReducerNameException : SpacetimeDBArgumentException
    {
        public SpacetimeDBEmptyReducerNameException()
            : base("Empty reducer name received from server", (string?)null)
        {
            HResult = SPACETIMEDB_EMPTY_REDUCER_NAME;
        }

        public SpacetimeDBEmptyReducerNameException(string? paramName)
            : base("Empty reducer name received from server", paramName)
        {
            HResult = SPACETIMEDB_EMPTY_REDUCER_NAME;
        }

        public SpacetimeDBEmptyReducerNameException(string? message, string? paramName)
            : base(message, paramName)
        {
            HResult = SPACETIMEDB_EMPTY_REDUCER_NAME;
        }

        public SpacetimeDBEmptyReducerNameException(string? message, Exception? innerException)
            : base(message, null, innerException)
        {
            HResult = SPACETIMEDB_EMPTY_REDUCER_NAME;
        }

        public SpacetimeDBEmptyReducerNameException(
            string? message,
            string? paramName,
            Exception? innerException
        )
            : base(message, paramName, innerException)
        {
            HResult = SPACETIMEDB_EMPTY_REDUCER_NAME;
        }

        protected SpacetimeDBEmptyReducerNameException(
            SerializationInfo info,
            StreamingContext context
        )
            : base(info, context) { }
    }
}
