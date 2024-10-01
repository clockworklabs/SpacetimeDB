/*  SpacetimeDB logging for Unity
 *  *  This class is only used in Unity projects.
 *  *
 */
#if UNITY_5_3_OR_NEWER
using System;
using UnityEngine;

namespace SpacetimeDB
{
    internal class UnityDebugLogger : ISpacetimeDBLogger
    {
        public void Debug(string message)
        {
            Debug.Log(message);
        }

        public void Trace(string message)
        {
            Debug.Log(message);
        }

        public void Info(string message)
        {
            Debug.Log(message);
        }

        public void Warn(string message)
        {
            Debug.LogWarning(message);
        }

        public void Error(string message)
        {
            Debug.LogError(message);
        }

        public void Exception(string message)
        {
            Debug.LogError(message);
        }

        public void Exception(Exception e)
        {
            Debug.LogException(e);
        }
    }
}
#endif
