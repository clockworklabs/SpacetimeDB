using Godot;
#if GODOT_PC
#nullable enable

namespace ImGuiGodot;

public partial class ImGuiLayer : CanvasLayer
{
    private Rid _subViewportRid;
    private Vector2I _subViewportSize = Vector2I.Zero;
    private Rid _canvasItem;
    private Transform2D _finalTransform = Transform2D.Identity;
    private bool _visible = true;
    private Viewport _parentViewport = null!;

    public override void _EnterTree()
    {
        Name = "ImGuiLayer";
        Layer = Internal.State.Instance.LayerNum;

        _parentViewport = GetViewport();
        _subViewportRid = AddLayerSubViewport(this);
        _canvasItem = RenderingServer.CanvasItemCreate();
        RenderingServer.CanvasItemSetParent(_canvasItem, GetCanvas());

        Internal.State.Instance.Renderer.InitViewport(_subViewportRid);
        Internal.State.Instance.Viewports.SetMainWindow(GetWindow(), _subViewportRid);
    }

    public override void _Ready()
    {
        VisibilityChanged += OnChangeVisibility;
        OnChangeVisibility();
    }

    public override void _ExitTree()
    {
        RenderingServer.FreeRid(_canvasItem);
        RenderingServer.FreeRid(_subViewportRid);
    }

    private void OnChangeVisibility()
    {
        _visible = Visible;
        if (_visible)
        {
            SetProcessInput(true);
        }
        else
        {
            SetProcessInput(false);
            Internal.State.Instance.Renderer.OnHide();
            _subViewportSize = Vector2I.Zero;
            RenderingServer.CanvasItemClear(_canvasItem);
        }
    }

    public override void _Input(InputEvent @event)
    {
        if (Internal.State.Instance.Input.ProcessInput(@event))
        {
            _parentViewport.SetInputAsHandled();
        }
    }

    public Vector2I UpdateViewport()
    {
        Vector2I vpSize = _parentViewport is Window w ? w.Size
            : (_parentViewport as SubViewport)?.Size
            ?? throw new System.InvalidOperationException();

        if (_visible)
        {
            var ft = _parentViewport.GetFinalTransform();
            if (_subViewportSize != vpSize || _finalTransform != ft)
            {
                // this is more or less how SubViewportContainer works
                _subViewportSize = vpSize;
                _finalTransform = ft;
                RenderingServer.ViewportSetSize(
                    _subViewportRid,
                    _subViewportSize.X,
                    _subViewportSize.Y);
                Rid vptex = RenderingServer.ViewportGetTexture(_subViewportRid);
                RenderingServer.CanvasItemClear(_canvasItem);
                RenderingServer.CanvasItemSetTransform(_canvasItem, ft.AffineInverse());
                RenderingServer.CanvasItemAddTextureRect(
                    _canvasItem,
                    new(0, 0, _subViewportSize.X, _subViewportSize.Y),
                    vptex);
            }
        }

        return vpSize;
    }

    private static Rid AddLayerSubViewport(Node parent)
    {
        Rid svp = RenderingServer.ViewportCreate();
        RenderingServer.ViewportSetTransparentBackground(svp, true);
        RenderingServer.ViewportSetUpdateMode(svp, RenderingServer.ViewportUpdateMode.Always);
        RenderingServer.ViewportSetClearMode(svp, RenderingServer.ViewportClearMode.Always);
        RenderingServer.ViewportSetActive(svp, true);
        RenderingServer.ViewportSetParentViewport(svp, parent.GetWindow().GetViewportRid());
        return svp;
    }
}
#else
namespace ImGuiNET
{
}

namespace ImGuiGodot
{
}
#endif
