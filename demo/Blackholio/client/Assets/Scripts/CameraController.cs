using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class CameraController : MonoBehaviour
{
	private void LateUpdate()
    {
        if (PlayerController.Local == null || !ConnectionManager.IsConnected())
        {
            return;
        }

        var centerOfMass = PlayerController.Local.CenterOfMass();
        if (centerOfMass.HasValue)
        {
            transform.position = new Vector3
            {
                x = centerOfMass.Value.x,
                y = centerOfMass.Value.y,
                z = transform.position.z
            };
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
