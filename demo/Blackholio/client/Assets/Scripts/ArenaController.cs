using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.Types;
using UnityEngine;

public class ArenaController : MonoBehaviour
{
    public SpriteRenderer backgroundInstance;
    public float borderThickness = 10;
    public Material borderMaterial;
    public ParallaxBackground starBackgroundPrefab;

    private void Start()
    {
        ConnectionManager.Conn.Db.Config.OnInsert += (ctx, value) =>
        {
            var worldSize = value.WorldSize;
            CreateBorderCube(new Vector2(worldSize / 2.0f, worldSize + borderThickness / 2),
                new Vector2(worldSize + borderThickness * 2.0f, borderThickness)); //North
			CreateBorderCube(new Vector2(worldSize / 2.0f, -borderThickness / 2),
				new Vector2(worldSize + borderThickness * 2.0f, borderThickness)); //South
			CreateBorderCube(new Vector2(worldSize + borderThickness / 2, worldSize / 2.0f),
				new Vector2(borderThickness, worldSize + borderThickness * 2.0f)); //East
			CreateBorderCube(new Vector2(-borderThickness / 2, worldSize / 2.0f),
				new Vector2(borderThickness, worldSize + borderThickness * 2.0f)); //West

            backgroundInstance.gameObject.SetActive(true); ;
            var size = worldSize / backgroundInstance.transform.localScale.x;
            backgroundInstance.size = new Vector2(size, size);
            backgroundInstance.transform.position = new Vector3((float)worldSize / 2, (float)worldSize / 2);
            
            // Start the camera in the middle of the screen for setup, but only if we have no player
            if (PlayerController.Local == null)
            {
                Camera.main.transform.position = new Vector3((float)worldSize / 2, (float)worldSize / 2, -10.0f);
            }
        };
    }

    private void CreateBorderCube(Vector2 position, Vector2 scale)
	{
		var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
        cube.name = "Border";
		cube.transform.localScale = new Vector3(scale.x, scale.y, 1);
		cube.transform.position = new Vector3(position.x, position.y, 1);
		cube.GetComponent<MeshRenderer>().material = borderMaterial;
	}
}
