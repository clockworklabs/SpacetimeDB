/*  SpacetimeDB logging for Unity
 *  *  This class is only used in Unity projects.
 *  *
 */
#if UNITY_5_3_OR_NEWER
using System;

namespace SpacetimeDB
{
    internal class UnityDebugLogger : ISpacetimeDBLogger
    {
        public void Debug(string message) =>
            UnityEngine.Debug.Log(message);

        public void Trace(string message) =>
            UnityEngine.Debug.Log(message);

        public void Info(string message) =>
            UnityEngine.Debug.Log(message);

        public void Warn(string message) =>
            UnityEngine.Debug.LogWarning(message);

        public void Error(string message) =>
            UnityEngine.Debug.LogError(message);

        public void Exception(string message) =>
            UnityEngine.Debug.LogError(message);

        public void Exception(Exception e) =>
            UnityEngine.Debug.LogException(e);
    }
}
#endif
