/*  SpacetimeDB logging for Unity
 *  *  This class is only used in Unity projects.
 *  *
 */
#if UNITY_5_3_OR_NEWER
using System;
using UnityEngine;

namespace SpacetimeDB
{
    public class UnityDebugLogger : ISpacetimeDBLogger
    {
        public void Log(string message)
        {
            Debug.Log(message);
        }

        public void LogError(string message)
        {
            Debug.LogError(message);
        }

        public void LogException(Exception e)
        {
            Debug.LogException(e);
        }

        public void LogWarning(string message)
        {
            Debug.LogWarning(message);
        }
    }
}
#endif
