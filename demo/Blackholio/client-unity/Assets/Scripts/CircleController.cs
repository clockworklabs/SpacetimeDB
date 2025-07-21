using System;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class CircleController : EntityController
{
	public static Color[] ColorPalette = new[]
	{
        //Yellow
		(Color)new Color32(175, 159, 49, 255),
		(Color)new Color32(175, 116, 49, 255),
        
        //Purple
        (Color)new Color32(112, 47, 252, 255),
		(Color)new Color32(51, 91, 252, 255),
        
        //Red
        (Color)new Color32(176, 54, 54, 255),
		(Color)new Color32(176, 109, 54, 255),
		(Color)new Color32(141, 43, 99, 255),
        
        //Blue
        (Color)new Color32(2, 188, 250, 255),
		(Color)new Color32(7, 50, 251, 255),
		(Color)new Color32(2, 28, 146, 255),
	};

    private PlayerController Owner;

    public void Spawn(Circle circle, PlayerController owner)
    {
        base.Spawn(circle.EntityId);
		SetColor(ColorPalette[circle.PlayerId % ColorPalette.Length]);

        this.Owner = owner;
        GetComponentInChildren<TMPro.TextMeshProUGUI>().text = owner.Username;
    }

	public override void OnDelete(EventContext context)
	{
		base.OnDelete(context);
        Owner.OnCircleDeleted(this);
	}
}