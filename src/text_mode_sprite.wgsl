#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping
#endif

#import bevy_render::{
    maths::affine_to_square,
    view::View,
}

@group(0) @binding(0)
var<uniform> view: View;

struct VertexInput {
    @builtin(vertex_index) index: u32,
    @location(0) i_model_transpose_col0: vec4<f32>,
    @location(1) i_model_transpose_col1: vec4<f32>,
    @location(2) i_model_transpose_col2: vec4<f32>,
    @location(3) i_bg: vec4<f32>,
    @location(4) i_fg: vec4<f32>,
    @location(5) i_alpha: f32,
    @location(6) i_uv_offset_scale: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) bg: vec4<f32>,
    @location(2) @interpolate(flat) fg: vec4<f32>,
    @location(3) alpha: f32,
};

@vertex
fn vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let vertex_position = vec3<f32>(
        f32(in.index & 0x1u),
        f32((in.index & 0x2u) >> 1u),
        0.0
    );

    out.clip_position = view.view_proj * affine_to_square(mat3x4<f32>(
        in.i_model_transpose_col0,
        in.i_model_transpose_col1,
        in.i_model_transpose_col2,
    )) * vec4<f32>(vertex_position, 1.0);
    out.uv = vec2<f32>(vertex_position.xy) * in.i_uv_offset_scale.zw + in.i_uv_offset_scale.xy;
    out.bg = in.i_bg;
    out.fg = in.i_fg;
    out.alpha = in.i_alpha;

    return out;
}

@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

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
    color = tonemapping::tone_mapping(color, view.color_grading);
    #endif

    return color;
}