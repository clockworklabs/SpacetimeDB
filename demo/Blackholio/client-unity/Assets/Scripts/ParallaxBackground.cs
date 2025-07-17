using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class ParallaxBackground : MonoBehaviour
{
    public float Multiplier;

	private void LateUpdate()
	{
		var pos = Camera.main.transform.position * Multiplier;
		pos.z = 0;
		transform.position = pos;
	}
}
