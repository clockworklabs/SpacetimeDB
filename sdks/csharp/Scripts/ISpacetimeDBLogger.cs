using System;

namespace SpacetimeDB
{
    public interface ISpacetimeDBLogger
    {
        void Log(string message);
        void LogError(string message);
        void LogWarning(string message);
        void LogException(Exception e);
    }

    public static class Logger
    {
        public static ISpacetimeDBLogger Current =

#if UNITY_5_3_OR_NEWER
            new UnityDebugLogger();
#else
            new ConsoleLogger();
#endif

        public static void Log(string message) => Current.Log(message);
        public static void LogError(string message) => Current.LogError(message);
        public static void LogWarning(string message) => Current.LogWarning(message);
        public static void LogException(Exception e) => Current.LogException(e);
    }
}
