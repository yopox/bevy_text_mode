#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping
#endif

#import bevy_render::view

@group(0) @binding(0)
var<uniform> view: View;

struct VertexOutput {
    @location(0) uv: vec2<f32>,
    @location(1) bg: vec4<f32>,
    @location(2) fg: vec4<f32>,
    @location(3) alpha: f32,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vertex(
    @location(0) vertex_position: vec3<f32>,
    @location(1) vertex_uv: vec2<f32>,
    @location(2) vertex_bg: vec4<f32>,
    @location(3) vertex_fg: vec4<f32>,
    @location(4) vertex_alpha: f32,
) -> VertexOutput {
    var out: VertexOutput;
    out.uv = vertex_uv;
    out.position = view.view_proj * vec4<f32>(vertex_position, 1.0);
    out.bg = vertex_bg;
    out.fg = vertex_fg;
    out.alpha = vertex_alpha;
    return out;
}

@group(1) @binding(0)
var sprite_texture: texture_2d<f32>;
@group(1) @binding(1)
var sprite_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(sprite_texture, sprite_sampler, in.uv);

    if (color[0] == 0.0) {
        color = in.bg;
        color[3] = in.alpha * in.bg[3];
    } else {
        color = in.fg;
        color[3] = in.alpha * in.fg[3];
    }

    #ifdef TONEMAP_IN_SHADER
    color = tone_mapping(color);
    #endif

    return color;
}