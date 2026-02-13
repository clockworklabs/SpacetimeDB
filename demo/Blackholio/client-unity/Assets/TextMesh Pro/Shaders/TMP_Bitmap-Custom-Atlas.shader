Shader "TextMeshPro/Bitmap Custom Atlas" {

Properties {
	_MainTex		("Font Atlas", 2D) = "white" {}
	_FaceTex		("Font Texture", 2D) = "white" {}
	[HDR]_FaceColor	("Text Color", Color) = (1,1,1,1)

	_VertexOffsetX	("Vertex OffsetX", float) = 0
	_VertexOffsetY	("Vertex OffsetY", float) = 0
	_MaskSoftnessX	("Mask SoftnessX", float) = 0
	_MaskSoftnessY	("Mask SoftnessY", float) = 0

	_ClipRect("Clip Rect", vector) = (-32767, -32767, 32767, 32767)
	_Padding		("Padding", float) = 0

	_StencilComp("Stencil Comparison", Float) = 8
	_Stencil("Stencil ID", Float) = 0
	_StencilOp("Stencil Operation", Float) = 0
	_StencilWriteMask("Stencil Write Mask", Float) = 255
	_StencilReadMask("Stencil Read Mask", Float) = 255

	_CullMode("Cull Mode", Float) = 0
	_ColorMask("Color Mask", Float) = 15
}

SubShader{

	Tags { "Queue" = "Transparent" "IgnoreProjector" = "True" "RenderType" = "Transparent" }

	Stencil
	{
		Ref[_Stencil]
		Comp[_StencilComp]
		Pass[_StencilOp]
		ReadMask[_StencilReadMask]
		WriteMask[_StencilWriteMask]
	}


	Lighting Off
	Cull [_CullMode]
	ZTest [unity_GUIZTestMode]
	ZWrite Off
	Fog { Mode Off }
	Blend SrcAlpha OneMinusSrcAlpha
	ColorMask[_ColorMask]

	Pass {
		CGPROGRAM
		#pragma vertex vert
		#pragma fragment frag

		#pragma multi_compile __ UNITY_UI_CLIP_RECT
		#pragma multi_compile __ UNITY_UI_ALPHACLIP


		#include "UnityCG.cginc"

		struct appdata_t {
			float4 vertex		: POSITION;
			fixed4 color		: COLOR;
			float2 texcoord0	: TEXCOORD0;
			float2 texcoord1	: TEXCOORD1;
		};

		struct v2f {
			float4	vertex		: SV_POSITION;
			fixed4	color		: COLOR;
			float2	texcoord0	: TEXCOORD0;
			float2	texcoord1	: TEXCOORD1;
			float4	mask		: TEXCOORD2;
		};

		uniform	sampler2D 	_MainTex;
		uniform	sampler2D 	_FaceTex;
		uniform float4		_FaceTex_ST;
		uniform	fixed4		_FaceColor;

		uniform float		_VertexOffsetX;
		uniform float		_VertexOffsetY;
		uniform float4		_ClipRect;
		uniform float		_MaskSoftnessX;
		uniform float		_MaskSoftnessY;

		float2 UnpackUV(float uv)
		{
			float2 output;
			output.x = floor(uv / 4096);
			output.y = uv - 4096 * output.x;

			return output * 0.001953125;
		}

		v2f vert (appdata_t v)
		{
			float4 vert = v.vertex;
			vert.x += _VertexOffsetX;
			vert.y += _VertexOffsetY;

			vert.xy += (vert.w * 0.5) / _ScreenParams.xy;

			float4 vPosition = UnityPixelSnap(UnityObjectToClipPos(vert));

			fixed4 faceColor = v.color;
			faceColor *= _FaceColor;

			v2f OUT;
			OUT.vertex = vPosition;
			OUT.color = faceColor;
			OUT.texcoord0 = v.texcoord0;
			OUT.texcoord1 = TRANSFORM_TEX(UnpackUV(v.texcoord1), _FaceTex);
			float2 pixelSize = vPosition.w;
			pixelSize /= abs(float2(_ScreenParams.x * UNITY_MATRIX_P[0][0], _ScreenParams.y * UNITY_MATRIX_P[1][1]));

			// Clamp _ClipRect to 16bit.
			float4 clampedRect = clamp(_ClipRect, -2e10, 2e10);
			OUT.mask = float4(vert.xy * 2 - clampedRect.xy - clampedRect.zw, 0.25 / (0.25 * half2(_MaskSoftnessX, _MaskSoftnessY) + pixelSize.xy));

			return OUT;
		}

		fixed4 frag (v2f IN) : SV_Target
		{
			fixed4 color = tex2D(_MainTex, IN.texcoord0) * tex2D(_FaceTex, IN.texcoord1) * IN.color;

			// Alternative implementation to UnityGet2DClipping with support for softness.
			#if UNITY_UI_CLIP_RECT
				half2 m = saturate((_ClipRect.zw - _ClipRect.xy - abs(IN.mask.xy)) * IN.mask.zw);
				color *= m.x * m.y;
			#endif

			#if UNITY_UI_ALPHACLIP
				clip(color.a - 0.001);
			#endif

			return color;
		}
		ENDCG
	}
}

	CustomEditor "TMPro.EditorUtilities.TMP_BitmapShaderGUI"
}
