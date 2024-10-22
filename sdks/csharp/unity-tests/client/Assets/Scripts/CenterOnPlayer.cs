using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class CenterOnPlayer : MonoBehaviour
{
    
    // Update is called once per frame
    void LateUpdate()
    {
        if (PlayerController.Local == null || !GameManager.IsConnected())
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
    }
}
