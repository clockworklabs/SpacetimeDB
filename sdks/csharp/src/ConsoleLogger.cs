using System;

namespace SpacetimeDB
{
    internal class ConsoleLogger : ISpacetimeDBLogger
    {
        [Flags]
        public enum LogLevel
        {
            None = 0,
            Debug = 1,
            Trace = 2,
            Info = 4,
            Warning = 8,
            Error = 16,
            Exception = 32,
            All = ~0
        }
        LogLevel _logLevel;

        public ConsoleLogger(LogLevel logLevel = LogLevel.All)
        {
            _logLevel = logLevel;
        }

        public void Debug(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Debug))
            {
                Console.WriteLine($"[D] {message}");
            }
        }

        public void Trace(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Trace))
            {
                Console.WriteLine($"[T] {message}");
            }
        }

        public void Info(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Info))
            {
                Console.WriteLine($"[I] {message}");
            }
        }

        public void Warn(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Warning))
            {
                Console.WriteLine($"[W] {message}");
            }
        }

        public void Error(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Error))
            {
                Console.WriteLine($"[E] {message}");
            }
        }

        public void Exception(string message)
        {
            if (_logLevel.HasFlag(LogLevel.Exception))
            {
                Console.WriteLine($"[X] {message}");
            }
        }

        public void Exception(Exception exception)
        {
            if (_logLevel.HasFlag(LogLevel.Exception))
            {
                Console.WriteLine($"[X] {exception}");
            }
        }
    }
}
