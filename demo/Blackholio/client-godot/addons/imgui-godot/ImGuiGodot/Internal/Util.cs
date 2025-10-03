using Godot;
using System.Runtime.CompilerServices;

namespace ImGuiGodot.Internal;

internal static class Util
{
    [UnsafeAccessor(UnsafeAccessorKind.Constructor)]
    public static extern Rid ConstructRid(ulong id);
}
