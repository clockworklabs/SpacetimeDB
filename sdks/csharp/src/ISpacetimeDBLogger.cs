using System;

namespace SpacetimeDB
{
    public interface ISpacetimeDBLogger
    {
        void Debug(string message);
        void Trace(string message);
        void Info(string message);
        void Warn(string message);
        void Error(string message);
        void Exception(string message);
        void Exception(Exception e);
    }

    public static class Log
    {
        public static ISpacetimeDBLogger Current =

#if UNITY_5_3_OR_NEWER
            new UnityDebugLogger();
#else
            new ConsoleLogger();
#endif

        public static void Debug(string message) => Current.Debug(message);
        public static void Trace(string message) => Current.Trace(message);
        public static void Info(string message) => Current.Info(message);
        public static void Warn(string message) => Current.Warn(message);
        public static void Error(string message) => Current.Error(message);
        public static void Exception(string message) => Current.Exception(message);
        public static void Exception(Exception exception) => Current.Exception(exception);
    }
}
