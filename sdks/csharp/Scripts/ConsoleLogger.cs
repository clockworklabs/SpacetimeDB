using System;

namespace SpacetimeDB
{
    public class ConsoleLogger : ISpacetimeDBLogger
    {
        [Flags]
        public enum LogLevel
        {
            None = 0,
            Debug = 1,
            Warning = 2,
            Error = 4,
            Exception = 8,
            All = Debug | Warning | Error | Exception
        }
        LogLevel _logLevel;

        public ConsoleLogger(LogLevel logLevel = LogLevel.All)
        {
            _logLevel = logLevel;
        }

        public void Log(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Debug))
            {
                Console.WriteLine(message);
            }
        }

        public void LogError(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Error))
            {
                Console.WriteLine($"Error: {message}");
            }
        }

        public void LogException(Exception e)
        {
            if (_logLevel.HasFlag(LogLevel.Exception))
            {
                Console.WriteLine($"Exception: {e.Message}");
            }
        }

        public void LogWarning(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Warning))
            {
                Console.WriteLine($"Warning: {message}");
            }
        }
    }
}
