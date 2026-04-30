using Godot;

public partial class CameraController : Camera2D
{
    [Export]
    public float BaseVisibleRadius { get; set; } = 50.0f;

    [Export]
    public float FollowLerpSpeed { get; set; } = 8.0f;

    [Export]
    public float ZoomLerpSpeed { get; set; } = 2.0f;
	
    private float WorldSize { get; }

    public CameraController(float worldSize)
    {
        WorldSize = worldSize;
    }

    public override void _Process(double delta)
    {
        Vector2 targetPosition;
        if (GameManager.IsConnected() && PlayerController.Local != null && PlayerController.Local.TryGetCenterOfMass(out var centerOfMass))
        {
            targetPosition = centerOfMass;
        }
        else
        {
            var hWorldSize = WorldSize * 0.5f;
            targetPosition = new Vector2(hWorldSize, hWorldSize);
        }

        GlobalPosition = GlobalPosition.Lerp(targetPosition, (float)delta * FollowLerpSpeed);

        if (PlayerController.Local == null)
        {
            return;
        }

        var targetCameraSize = CalculateCameraSize(PlayerController.Local);
        var desiredZoom = Vector2.One * (BaseVisibleRadius / Mathf.Max(targetCameraSize, 1.0f));
        Zoom = Zoom.Lerp(desiredZoom, (float)delta * ZoomLerpSpeed);
    }

    private static float CalculateCameraSize(PlayerController player) => 10.0f
                                                                         + Mathf.Min(10.0f, player.TotalMass() / 5.0f)
                                                                         + Mathf.Min(player.NumberOfOwnedCircles - 1, 1) * 30.0f;
}