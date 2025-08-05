#if GODOT_PC
using Godot;
using System;

namespace ImGuiGodot.Internal;

internal interface IRenderer : IDisposable
{
    string Name { get; }
    void InitViewport(Rid vprid);
    void CloseViewport(Rid vprid);
    void Render();
    void OnHide();
}
#endif
