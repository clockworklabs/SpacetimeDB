shader_type canvas_item;

uniform float border_size : hint_range(0.0, 0.5) = 0.001;
uniform vec4 border_color : source_color = vec4(1.0, 1.0, 1.0, 1.0);
uniform vec4 background_color : source_color = vec4(0.0, 0.0, 0.0, 1.0);

void fragment() {
    vec2 uv = UV;
    // Check if we're within the border region
    bool is_border = 
        uv.x < border_size ||
        uv.x > 1.0 - border_size ||
        uv.y < border_size ||
        uv.y > 1.0 - border_size;

    if (is_border) {
        COLOR = border_color;
    } else {
		COLOR = background_color - texture(TEXTURE, uv);
    }
}
