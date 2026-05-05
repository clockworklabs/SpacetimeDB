/*  SpacetimeDB logging for Godot
 *  *  This class is only used in Godot projects.
 *  *
 */
#if GODOT
using System;
using Godot;

namespace SpacetimeDB
{
    internal class GodotDebugLogger : ISpacetimeDBLogger
    {
        public void Debug(string message) =>
            GD.Print(message);

        public void Trace(string message) =>
            GD.Print(message);

        public void Info(string message) =>
            GD.Print(message);

        public void Warn(string message) =>
            GD.PushWarning(message);

        public void Error(string message) =>
            GD.PrintErr(message);

        public void Exception(string message) =>
            GD.PrintErr(message);

        public void Exception(Exception e) =>
            GD.PushError(e);
    }
}
#endif
