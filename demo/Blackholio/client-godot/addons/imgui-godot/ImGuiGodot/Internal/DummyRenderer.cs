#if GODOT_PC
using Godot;

namespace ImGuiGodot.Internal;

internal sealed class DummyRenderer : IRenderer
{
    public string Name => "godot4_net_dummy";

    public void InitViewport(Rid vprid)
    {
    }

    public void CloseViewport(Rid vprid)
    {
    }

    public void OnHide()
    {
    }

    public void Render()
    {
    }

    public void Dispose()
    {
    }
}
#endif
