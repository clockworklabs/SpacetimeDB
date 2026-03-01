#if GODOT_PC
using Godot;
using ImGuiNET;

namespace ImGuiGodot.Internal;

internal sealed class InputLocal : Input
{
    protected override void UpdateMousePos(ImGuiIOPtr io)
    {
        // do not use global mouse position
    }

    public override bool ProcessInput(InputEvent evt)
    {
        // no support for SubViewport widgets

        if (evt is InputEventMouseMotion mm)
        {
            var io = ImGui.GetIO();
            var mousePos = mm.Position;
#pragma warning disable IDE0004 // Remove Unnecessary Cast
            io.AddMousePosEvent((float)mousePos.X, (float)mousePos.Y);
#pragma warning restore IDE0004 // Remove Unnecessary Cast
            mm.Dispose();
            return io.WantCaptureMouse;
        }
        return HandleEvent(evt);
    }
}
#endif
