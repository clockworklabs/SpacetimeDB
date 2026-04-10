#if GODOT_PC
#nullable enable
using Godot;
using ImGuiNET;
using System;
using Vector2 = System.Numerics.Vector2;

namespace ImGuiGodot.Internal;

internal sealed class BackendNet : IBackend
{
    public float JoyAxisDeadZone
    {
        get => State.Instance.JoyAxisDeadZone;
        set => State.Instance.JoyAxisDeadZone = value;
    }

    public float Scale
    {
        get => State.Instance.Scale;
        set => State.Instance.Scale = value;
    }

    public bool Visible
    {
        get => State.Instance.Layer.Visible;
        set => State.Instance.Layer.Visible = value;
    }

    public void AddFont(FontFile fontData, int fontSize, bool merge, ushort[]? glyphRanges)
    {
        State.Instance.Fonts.AddFont(fontData, fontSize, merge, glyphRanges);
    }

    public void AddFontDefault()
    {
        State.Instance.Fonts.AddFont(null, 13, false, null);
    }

    public void Connect(Callable callable)
    {
        ImGuiController.Instance?.Signaler.Connect("imgui_layout", callable);
    }

    public void RebuildFontAtlas()
    {
        if (State.Instance.InProcessFrame)
            throw new InvalidOperationException("fonts can't be changed during process");

        bool scaleToDpi = (bool)ProjectSettings.GetSetting("display/window/dpi/allow_hidpi");
        int dpiFactor = Math.Max(1, DisplayServer.ScreenGetDpi() / 96);
        State.Instance.Fonts.RebuildFontAtlas(scaleToDpi ? dpiFactor * Scale : Scale);
    }

    public void ResetFonts()
    {
        State.Instance.Fonts.ResetFonts();
    }

    public void SetIniFilename(string filename)
    {
        State.Instance.SetIniFilename(filename);
    }

    public void SetMainViewport(Viewport vp)
    {
        ImGuiController.Instance.SetMainViewport(vp);
    }

    public bool SubViewportWidget(SubViewport svp)
    {
        Vector2 vpSize = new(svp.Size.X, svp.Size.Y);
        var pos = ImGui.GetCursorScreenPos();
        var pos_max = new Vector2(pos.X + vpSize.X, pos.Y + vpSize.Y);
        ImGui.GetWindowDrawList().AddImage((IntPtr)svp.GetTexture().GetRid().Id, pos, pos_max);

        ImGui.PushID(svp.NativeInstance);
        ImGui.InvisibleButton("godot_subviewport", vpSize);
        ImGui.PopID();

        if (ImGui.IsItemHovered())
        {
            State.Instance.Input.CurrentSubViewport = svp;
            State.Instance.Input.CurrentSubViewportPos = pos;
            return true;
        }
        return false;
    }
}
#endif
