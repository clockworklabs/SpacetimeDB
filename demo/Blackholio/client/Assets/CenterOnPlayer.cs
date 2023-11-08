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
        var position = transform.position;
        position.x = PlayerController.Local.transform.position.x;
        position.y = PlayerController.Local.transform.position.y;
        transform.position = position;
    }
}
