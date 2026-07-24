using System;
#if UNITY_5_3_OR_NEWER
using UnityEngine;
#endif

namespace SpacetimeDB
{
    internal interface ISpacetimeDBLogger
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
        internal static ISpacetimeDBLogger Current =

#if UNITY_5_3_OR_NEWER
            new UnityDebugLogger();
#elif GODOT
            new GodotDebugLogger();
#else
            new ConsoleLogger();
#endif

        /// <summary>
        /// Resets the static instance to prevent data persistence when Enter Play Mode Options (Disable Domain Reloading) is active.
        /// RuntimeInitializeOnLoadMethod is used since it is supported in older versions of Unity.
        /// AutoStaticsCleanup and NoAutoStaticsCleanup is only supported in Unity 6+
        /// </summary>
        /// <remarks>
        /// See the <see href="https://docs.unity3d.com/6000.5/Documentation/Manual/domain-reloading.html">Unity Domain Reloading Manual</see> 
        /// and the <see href="https://docs.unity3d.com/6000.5/Documentation/ScriptReference/RuntimeInitializeOnLoadMethodAttribute.html">RuntimeInitializeOnLoadMethodAttribute API Docs</see> for details.
        /// </remarks>
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        private static void ResetStaticFields()
        {
            Current = new UnityDebugLogger();
        }

        public static void Debug(string message) => Current.Debug(message);
        public static void Trace(string message) => Current.Trace(message);
        public static void Info(string message) => Current.Info(message);
        public static void Warn(string message) => Current.Warn(message);
        public static void Error(string message) => Current.Error(message);
        public static void Exception(string message) => Current.Exception(message);
        public static void Exception(Exception exception) => Current.Exception(exception);
    }
}
