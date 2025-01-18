using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class CameraController : MonoBehaviour
{
    public static float WorldSize = 0.0f;

	private void LateUpdate()
    {
        var arenaCenterTransform = new Vector3(WorldSize / 2, WorldSize / 2, -10.0f);
        if (PlayerController.Local == null || !GameManager.IsConnected())
        {
            // Set the camera to be in middle of the arena if we are not connected or 
            // there is no local player
            transform.position = arenaCenterTransform;
            return;
        }

        var centerOfMass = PlayerController.Local.CenterOfMass();
        if (centerOfMass.HasValue)
        {
            // Set the camera to be the center of mass of the local player
            // if the local player has one
            transform.position = new Vector3
            {
                x = centerOfMass.Value.x,
                y = centerOfMass.Value.y,
                z = transform.position.z
            };
        } else {
            transform.position = arenaCenterTransform;
        }

		float targetCameraSize = CalculateCameraSize(PlayerController.Local);
		Camera.main.orthographicSize = Mathf.Lerp(Camera.main.orthographicSize, targetCameraSize, Time.deltaTime * 2);
	}

	private float CalculateCameraSize(PlayerController player)
	{
		return 50f + //Base size
            Mathf.Min(50, player.TotalMass() / 5) + //Increase camera size with mass
            Mathf.Min(player.NumberOfOwnedCircles - 1, 1) * 30; //Zoom out when player splits
	}
}
