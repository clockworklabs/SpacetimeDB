float2 UnpackUV(float uv)
{ 
	float2 output;
	output.x = floor(uv / 4096);
	output.y = uv - 4096 * output.x;

	return output * 0.001953125;
}

fixed4 GetColor(half d, fixed4 faceColor, fixed4 outlineColor, half outline, half softness)
{
	half faceAlpha = 1-saturate((d - outline * 0.5 + softness * 0.5) / (1.0 + softness));
	half outlineAlpha = saturate((d + outline * 0.5)) * sqrt(min(1.0, outline));

	faceColor.rgb *= faceColor.a;
	outlineColor.rgb *= outlineColor.a;

	faceColor = lerp(faceColor, outlineColor, outlineAlpha);

	faceColor *= faceAlpha;

	return faceColor;
}

float3 GetSurfaceNormal(float4 h, float bias)
{
	bool raisedBevel = step(1, fmod(_ShaderFlags, 2));

	h += bias+_BevelOffset;

	float bevelWidth = max(.01, _OutlineWidth+_BevelWidth);

  // Track outline
	h -= .5;
	h /= bevelWidth;
	h = saturate(h+.5);

	if(raisedBevel) h = 1 - abs(h*2.0 - 1.0);
	h = lerp(h, sin(h*3.141592/2.0), _BevelRoundness);
	h = min(h, 1.0-_BevelClamp);
	h *= _Bevel * bevelWidth * _GradientScale * -2.0;

	float3 va = normalize(float3(1.0, 0.0, h.y - h.x));
	float3 vb = normalize(float3(0.0, -1.0, h.w - h.z));

	return cross(va, vb);
}

float3 GetSurfaceNormal(float2 uv, float bias, float3 delta)
{
	// Read "height field"
  float4 h = {tex2D(_MainTex, uv - delta.xz).a,
				tex2D(_MainTex, uv + delta.xz).a,
				tex2D(_MainTex, uv - delta.zy).a,
				tex2D(_MainTex, uv + delta.zy).a};

	return GetSurfaceNormal(h, bias);
}

float3 GetSpecular(float3 n, float3 l)
{
	float spec = pow(max(0.0, dot(n, l)), _Reflectivity);
	return _SpecularColor.rgb * spec * _SpecularPower;
}

float4 GetGlowColor(float d, float scale)
{
	float glow = d - (_GlowOffset*_ScaleRatioB) * 0.5 * scale;
	float t = lerp(_GlowInner, (_GlowOuter * _ScaleRatioB), step(0.0, glow)) * 0.5 * scale;
	glow = saturate(abs(glow/(1.0 + t)));
	glow = 1.0-pow(glow, _GlowPower);
	glow *= sqrt(min(1.0, t)); // Fade off glow thinner than 1 screen pixel
	return float4(_GlowColor.rgb, saturate(_GlowColor.a * glow * 2));
}

float4 BlendARGB(float4 overlying, float4 underlying)
{
	overlying.rgb *= overlying.a;
	underlying.rgb *= underlying.a;
	float3 blended = overlying.rgb + ((1-overlying.a)*underlying.rgb);
	float alpha = underlying.a + (1-underlying.a)*overlying.a;
	return float4(blended, alpha);
}

