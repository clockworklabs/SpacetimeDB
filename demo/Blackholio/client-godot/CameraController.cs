using Godot;

public partial class CameraController : Camera2D
{
    public static float WorldSize { get; set; }

    [Export]
    public float BaseVisibleRadius { get; set; } = 50.0f;

    [Export]
    public float FollowLerpSpeed { get; set; } = 8.0f;

    [Export]
    public float ZoomLerpSpeed { get; set; } = 2.0f;

    public override void _Process(double delta)
    {
        var arenaCenter = new Vector2(WorldSize / 2.0f, WorldSize / 2.0f);
        var targetPosition = arenaCenter;

        if (PlayerController.Local != null && GameManager.IsConnected())
        {
            var centerOfMass = PlayerController.Local.CenterOfMass();
            if (centerOfMass.HasValue)
            {
                targetPosition = centerOfMass.Value;
            }
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

    private float CalculateCameraSize(PlayerController player)
    {
        return 10.0f
               + Mathf.Min(10.0f, player.TotalMass() / 5.0f)
               + Mathf.Min(player.NumberOfOwnedCircles - 1, 1) * 30.0f;
    }
}
