using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class CenterOnPlayer : MonoBehaviour
{
    // Update is called once per frame
    void LateUpdate()
    {
        if (PlayerController.Local == null)
        {
            return;
        }

        var centerOfMass = PlayerController.Local.CalculateCenterOfMass();
        transform.position = new Vector3
        {
            x = centerOfMass.x,
            y = centerOfMass.y,
            z = transform.position.z
        };
    }
}
