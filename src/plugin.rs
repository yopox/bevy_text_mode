use std::f32::consts::PI;
use std::ops::Range;

use bevy::asset::{
    embedded_asset, load_embedded_asset, AssetEvent, AssetId, AssetServer, Assets, Handle,
};
use bevy::color::{ColorToComponents, LinearRgba};
use bevy::core_pipeline::core_2d::{Transparent2d, CORE_2D_DEPTH_FORMAT};
use bevy::core_pipeline::tonemapping::{
    get_lut_bind_group_layout_entries, get_lut_bindings, DebandDither, Tonemapping, TonemappingLuts,
};
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::{lifetimeless::*, SystemParamItem};
use bevy::image::{Image, TextureAtlasLayout};
use bevy::math::{Affine3A, FloatOrd, Rect, Vec2, Vec4};
use bevy::mesh::VertexBufferLayout;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::*;
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::{RenderEntity, SyncToRenderWorld};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::view::{
    texture_format_from_code, texture_format_to_code, ExtractedView, Msaa, RenderVisibleEntities,
    RetainedViewEntity, ViewUniform, ViewUniformOffset, ViewUniforms,
};
use bevy::render::{
    Extract, ExtractSchedule, GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems,
};
use bevy::shader::{Shader, ShaderDefVal};
use bevy::sprite_render::{queue_material2d_meshes, ColorMaterial, SpriteSystems};
use bevy::transform::components::GlobalTransform;
use bytemuck::{Pod, Zeroable};
use fixedbitset::FixedBitSet;

use crate::computed_text_mode_slices::{
    compute_text_mode_slices_on_asset_event, compute_text_mode_slices_on_sprite_change,
    ComputedTextModeTextureSlices,
};
use crate::TextModeSprite;

pub struct TextModePlugin;

impl Plugin for TextModePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "text_mode_sprite.wgsl");

        app.register_required_components::<TextModeSprite, SyncToRenderWorld>();

        app.add_systems(
            PostUpdate,
            (
                compute_text_mode_slices_on_asset_event.before(bevy::asset::AssetEventSystems),
                compute_text_mode_slices_on_sprite_change,
            )
                .in_set(SpriteSystems::ComputeSlices),
        );

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<TextModeImageBindGroups>()
                .init_gpu_resource::<SpecializedRenderPipelines<TextModeSpritePipeline>>()
                .init_resource::<TextModeSpriteMeta>()
                .init_resource::<ExtractedTextModeSprites>()
                .init_resource::<ExtractedTextModeSlices>()
                .init_resource::<TextModeSpriteAssetEvents>()
                .init_resource::<TextModeSpriteBatches>()
                .add_render_command::<Transparent2d, DrawTextModeSprite>()
                .add_systems(RenderStartup, init_text_mode_sprite_pipeline)
                .add_systems(
                    ExtractSchedule,
                    (
                        extract_text_mode_sprites.in_set(SpriteSystems::ExtractSprites),
                        extract_text_mode_sprite_events,
                    ),
                )
                .add_systems(
                    Render,
                    (
                        queue_text_mode_sprites
                            .in_set(RenderSystems::Queue)
                            .ambiguous_with(queue_material2d_meshes::<ColorMaterial>),
                        prepare_text_mode_sprite_image_bind_groups
                            .in_set(RenderSystems::PrepareBindGroups),
                        prepare_text_mode_sprite_view_bind_groups
                            .in_set(RenderSystems::PrepareBindGroups),
                    ),
                );
        };
    }
}

#[derive(Resource)]
pub struct TextModeSpritePipeline {
    view_layout: BindGroupLayoutDescriptor,
    material_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
}

pub fn init_text_mode_sprite_pipeline(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tonemapping_lut_entries = get_lut_bind_group_layout_entries();
    let view_layout = BindGroupLayoutDescriptor::new(
        "text_mode_sprite_view_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                uniform_buffer::<ViewUniform>(true),
                tonemapping_lut_entries[0].visibility(ShaderStages::FRAGMENT),
                tonemapping_lut_entries[1].visibility(ShaderStages::FRAGMENT),
            ),
        ),
    );

    let material_layout = BindGroupLayoutDescriptor::new(
        "text_mode_sprite_material_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    );

    commands.insert_resource(TextModeSpritePipeline {
        view_layout,
        material_layout,
        shader: load_embedded_asset!(asset_server.as_ref(), "text_mode_sprite.wgsl"),
    });
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct TextModeSpritePipelineKey: u32 {
        const NONE                              = 0;
        const TONEMAP_IN_SHADER                 = 1 << 0;
        const DEBAND_DITHER                     = 1 << 1;
        const SRGB_COMPOSITING                  = 1 << 2;
        const OKLAB_COMPOSITING                 = 1 << 3;
        const COLOR_TARGET_FORMAT_RESERVED_BITS = Self::COLOR_TARGET_FORMAT_MASK_BITS << Self::COLOR_TARGET_FORMAT_SHIFT_BITS;
        const MSAA_RESERVED_BITS                = Self::MSAA_MASK_BITS << Self::MSAA_SHIFT_BITS;
        const TONEMAP_METHOD_RESERVED_BITS      = Self::TONEMAP_METHOD_MASK_BITS << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_NONE               = 0 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_REINHARD           = 1 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_REINHARD_LUMINANCE = 2 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_ACES_FITTED        = 3 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_AGX                = 4 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM = 5 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_TONY_MC_MAPFACE    = 6 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_BLENDER_FILMIC     = 7 << Self::TONEMAP_METHOD_SHIFT_BITS;
        const TONEMAP_METHOD_PBR_NEUTRAL        = 8 << Self::TONEMAP_METHOD_SHIFT_BITS;
    }
}

impl TextModeSpritePipelineKey {
    const COLOR_TARGET_FORMAT_MASK_BITS: u32 = bevy::render::view::COLOR_TARGET_FORMAT_MASK_BITS;
    const COLOR_TARGET_FORMAT_SHIFT_BITS: u32 = 4;
    const MSAA_MASK_BITS: u32 = 0b111;
    const MSAA_SHIFT_BITS: u32 = 32 - Self::MSAA_MASK_BITS.count_ones();
    const TONEMAP_METHOD_MASK_BITS: u32 = 0b1111;
    const TONEMAP_METHOD_SHIFT_BITS: u32 =
        Self::MSAA_SHIFT_BITS - Self::TONEMAP_METHOD_MASK_BITS.count_ones();

    #[inline]
    pub const fn from_msaa_samples(msaa_samples: u32) -> Self {
        let msaa_bits =
            (msaa_samples.trailing_zeros() & Self::MSAA_MASK_BITS) << Self::MSAA_SHIFT_BITS;
        Self::from_bits_retain(msaa_bits)
    }

    #[inline]
    pub const fn msaa_samples(&self) -> u32 {
        1 << ((self.bits() >> Self::MSAA_SHIFT_BITS) & Self::MSAA_MASK_BITS)
    }

    #[inline]
    pub fn from_target_format(format: TextureFormat) -> Self {
        let code = texture_format_to_code(format)
            .expect("Texture format is not supported by the pipeline") as u32;
        Self::from_bits_retain(
            (code & Self::COLOR_TARGET_FORMAT_MASK_BITS) << Self::COLOR_TARGET_FORMAT_SHIFT_BITS,
        )
    }

    #[inline]
    pub fn target_format(&self) -> TextureFormat {
        let code = ((self.bits() >> Self::COLOR_TARGET_FORMAT_SHIFT_BITS)
            & Self::COLOR_TARGET_FORMAT_MASK_BITS) as u8;
        texture_format_from_code(code)
            .expect("Unknown bits in `COLOR_TARGET_FORMAT_MASK_BITS` of the pipeline key")
    }
}

impl SpecializedRenderPipeline for TextModeSpritePipeline {
    type Key = TextModeSpritePipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs = Vec::new();

        if key.contains(TextModeSpritePipelineKey::TONEMAP_IN_SHADER) {
            shader_defs.push("TONEMAP_IN_SHADER".into());
            shader_defs.push(ShaderDefVal::UInt(
                "TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(),
                1,
            ));
            shader_defs.push(ShaderDefVal::UInt(
                "TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(),
                2,
            ));

            let method = key.intersection(TextModeSpritePipelineKey::TONEMAP_METHOD_RESERVED_BITS);

            if method == TextModeSpritePipelineKey::TONEMAP_METHOD_NONE {
                shader_defs.push("TONEMAP_METHOD_NONE".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_REINHARD {
                shader_defs.push("TONEMAP_METHOD_REINHARD".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_REINHARD_LUMINANCE {
                shader_defs.push("TONEMAP_METHOD_REINHARD_LUMINANCE".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_ACES_FITTED {
                shader_defs.push("TONEMAP_METHOD_ACES_FITTED".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_AGX {
                shader_defs.push("TONEMAP_METHOD_AGX".into());
            } else if method
                == TextModeSpritePipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM
            {
                shader_defs.push("TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_BLENDER_FILMIC {
                shader_defs.push("TONEMAP_METHOD_BLENDER_FILMIC".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE {
                shader_defs.push("TONEMAP_METHOD_TONY_MC_MAPFACE".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_PBR_NEUTRAL {
                shader_defs.push("TONEMAP_METHOD_PBR_NEUTRAL".into());
            }

            // Debanding is tied to tonemapping in the shader, cannot run without it.
            if key.contains(TextModeSpritePipelineKey::DEBAND_DITHER) {
                shader_defs.push("DEBAND_DITHER".into());
            }
        }

        if key.contains(TextModeSpritePipelineKey::SRGB_COMPOSITING) {
            shader_defs.push("SRGB_OUTPUT".into());
        }
        if key.contains(TextModeSpritePipelineKey::OKLAB_COMPOSITING) {
            shader_defs.push("OKLAB_OUTPUT".into());
        }

        let format = key.target_format();

        let instance_rate_vertex_buffer_layout = VertexBufferLayout {
            array_stride: 112,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                // @location(0) i_model_transpose_col0: vec4<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 0,
                },
                // @location(1) i_model_transpose_col1: vec4<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 1,
                },
                // @location(2) i_model_transpose_col2: vec4<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 2,
                },
                // @location(3) i_bg: vec4<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 3,
                },
                // @location(4) i_fg: vec4<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 64,
                    shader_location: 4,
                },
                // @location(5) i_alpha: f32,
                VertexAttribute {
                    format: VertexFormat::Float32,
                    offset: 80,
                    shader_location: 5,
                },
                // @location(6) i_uv_offset_scale: vec4<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 84,
                    shader_location: 6,
                },
                // @location(7) i_pad: vec3<f32>,
                VertexAttribute {
                    format: VertexFormat::Float32x3,
                    offset: 100,
                    shader_location: 7,
                },
            ],
        };

        RenderPipelineDescriptor {
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: shader_defs.clone(),
                buffers: vec![instance_rate_vertex_buffer_layout],
                ..default()
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            layout: vec![self.view_layout.clone(), self.material_layout.clone()],
            depth_stencil: Some(DepthStencilState {
                format: CORE_2D_DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            label: Some("text_mode_sprite_pipeline".into()),
            ..default()
        }
    }
}

pub struct TextModeExtractedSlice {
    pub offset: Vec2,
    pub rect: Rect,
    pub size: Vec2,
}

/// See [bevy::sprite_render::ExtractedSprite]
pub struct TextModeExtractedSprite {
    pub main_entity: Entity,
    pub render_entity: Entity,
    pub transform: GlobalTransform,
    pub bg: LinearRgba,
    pub fg: LinearRgba,
    pub alpha: f32,
    pub rotation: u8,
    pub image_handle_id: AssetId<Image>,
    pub flip_x: bool,
    pub flip_y: bool,
    pub kind: TextModeExtractedSpriteKind,
}

pub enum TextModeExtractedSpriteKind {
    /// A single sprite with custom sizing options
    Single {
        anchor: Vec2,
        rect: Option<Rect>,
        custom_size: Option<Vec2>,
    },
    /// Indexes into the list of [`TextModeExtractedSlice`]s stored in the [`ExtractedTextModeSlices`] resource
    Slices { indices: Range<usize> },
}

#[derive(Resource, Default)]
pub struct ExtractedTextModeSprites {
    pub sprites: Vec<TextModeExtractedSprite>,
}

#[derive(Resource, Default)]
pub struct ExtractedTextModeSlices {
    pub slices: Vec<TextModeExtractedSlice>,
}

#[derive(Resource, Default)]
pub struct TextModeSpriteAssetEvents {
    pub images: Vec<AssetEvent<Image>>,
}

pub fn extract_text_mode_sprite_events(
    mut events: ResMut<TextModeSpriteAssetEvents>,
    mut image_events: Extract<MessageReader<AssetEvent<Image>>>,
) {
    let TextModeSpriteAssetEvents { ref mut images } = *events;
    images.clear();

    for event in image_events.read() {
        images.push(*event);
    }
}

/// See [bevy::sprite_render::extract_sprites]
pub fn extract_text_mode_sprites(
    mut extracted_sprites: ResMut<ExtractedTextModeSprites>,
    mut extracted_slices: ResMut<ExtractedTextModeSlices>,
    texture_atlases: Extract<Res<Assets<TextureAtlasLayout>>>,
    sprite_query: Extract<
        Query<(
            Entity,
            RenderEntity,
            &ViewVisibility,
            &TextModeSprite,
            &GlobalTransform,
            Option<&ComputedTextModeTextureSlices>,
        )>,
    >,
) {
    extracted_sprites.sprites.clear();
    extracted_slices.slices.clear();
    for (main_entity, render_entity, view_visibility, sprite, transform, slices) in
        sprite_query.iter()
    {
        if !view_visibility.get() {
            continue;
        }

        if let Some(slices) = slices {
            let start = extracted_slices.slices.len();
            extracted_slices
                .slices
                .extend(slices.extract_text_mode_slices(sprite, sprite.anchor.as_vec()));
            let end = extracted_slices.slices.len();
            extracted_sprites.sprites.push(TextModeExtractedSprite {
                main_entity,
                render_entity,
                bg: sprite.bg,
                fg: sprite.fg,
                alpha: sprite.alpha,
                rotation: sprite.rotation,
                transform: *transform,
                flip_x: sprite.flip_x,
                flip_y: sprite.flip_y,
                image_handle_id: sprite.image.id(),
                kind: TextModeExtractedSpriteKind::Slices {
                    indices: start..end,
                },
            });
        } else {
            let atlas_rect = sprite
                .texture_atlas
                .as_ref()
                .and_then(|s| s.texture_rect(&texture_atlases).map(|r| r.as_rect()));
            let rect = match (atlas_rect, sprite.rect) {
                (None, None) => None,
                (None, Some(sprite_rect)) => Some(sprite_rect),
                (Some(atlas_rect), None) => Some(atlas_rect),
                (Some(atlas_rect), Some(mut sprite_rect)) => {
                    sprite_rect.min += atlas_rect.min;
                    sprite_rect.max += atlas_rect.min;
                    Some(sprite_rect)
                }
            };

            extracted_sprites.sprites.push(TextModeExtractedSprite {
                main_entity,
                render_entity,
                bg: sprite.bg,
                fg: sprite.fg,
                alpha: sprite.alpha,
                rotation: sprite.rotation,
                transform: *transform,
                flip_x: sprite.flip_x,
                flip_y: sprite.flip_y,
                image_handle_id: sprite.image.id(),
                kind: TextModeExtractedSpriteKind::Single {
                    anchor: sprite.anchor.as_vec(),
                    rect,
                    // Pass the custom size
                    custom_size: sprite.custom_size,
                },
            });
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TextModeSpriteInstance {
    pub i_model_transpose: [Vec4; 3],
    pub i_bg: [f32; 4],
    pub i_fg: [f32; 4],
    pub i_alpha: f32,
    pub i_uv: [f32; 4],
    pub i_pad: [f32; 3],
}

impl TextModeSpriteInstance {
    #[inline]
    fn from(
        transform: &Affine3A,
        bg: &LinearRgba,
        fg: &LinearRgba,
        alpha: f32,
        uv_offset_scale: &Vec4,
    ) -> Self {
        let transpose_model_3x3 = transform.matrix3.transpose();
        Self {
            i_model_transpose: [
                transpose_model_3x3.x_axis.extend(transform.translation.x),
                transpose_model_3x3.y_axis.extend(transform.translation.y),
                transpose_model_3x3.z_axis.extend(transform.translation.z),
            ],
            i_bg: bg.to_f32_array(),
            i_fg: fg.to_f32_array(),
            i_alpha: alpha,
            i_uv: uv_offset_scale.to_array(),
            i_pad: [0., 0., 0.],
        }
    }
}

/// See [bevy::sprite_render::SpriteMeta]
#[derive(Resource)]
pub struct TextModeSpriteMeta {
    sprite_index_buffer: RawBufferVec<u32>,
    sprite_instance_buffer: RawBufferVec<TextModeSpriteInstance>,
}

impl Default for TextModeSpriteMeta {
    fn default() -> Self {
        Self {
            sprite_index_buffer: RawBufferVec::<u32>::new(BufferUsages::INDEX),
            sprite_instance_buffer: RawBufferVec::<TextModeSpriteInstance>::new(
                BufferUsages::VERTEX,
            ),
        }
    }
}

#[derive(Component)]
pub struct TextModeSpriteViewBindGroup {
    pub value: BindGroup,
}

#[derive(Resource, Deref, DerefMut, Default)]
pub struct TextModeSpriteBatches(HashMap<(RetainedViewEntity, Entity), TextModeSpriteBatch>);

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TextModeSpriteBatch {
    image_handle_id: AssetId<Image>,
    range: Range<u32>,
}

#[derive(Resource, Default)]
pub struct TextModeImageBindGroups {
    values: HashMap<AssetId<Image>, BindGroup>,
}

/// See [bevy::sprite_render::queue_sprites]
#[allow(clippy::too_many_arguments)]
pub fn queue_text_mode_sprites(
    mut view_entities: Local<FixedBitSet>,
    draw_functions: Res<DrawFunctions<Transparent2d>>,
    sprite_pipeline: Res<TextModeSpritePipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<TextModeSpritePipeline>>,
    pipeline_cache: Res<PipelineCache>,
    extracted_sprites: Res<ExtractedTextModeSprites>,
    mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent2d>>,
    mut cameras: Query<(
        &RenderVisibleEntities,
        &ExtractedCamera,
        &ExtractedView,
        &Msaa,
        Option<&Tonemapping>,
        Option<&DebandDither>,
    )>,
) {
    let draw_sprite_function = draw_functions.read().id::<DrawTextModeSprite>();

    for (visible_entities, camera, view, msaa, tonemapping, dither) in &mut cameras {
        let Some(transparent_phase) = transparent_render_phases.get_mut(&view.retained_view_entity)
        else {
            continue;
        };

        let msaa_key = TextModeSpritePipelineKey::from_msaa_samples(msaa.samples());
        let mut view_key =
            TextModeSpritePipelineKey::from_target_format(view.target_format) | msaa_key;

        if camera
            .compositing_space
            .is_some_and(|s| s == bevy::camera::CompositingSpace::Srgb)
        {
            view_key |= TextModeSpritePipelineKey::SRGB_COMPOSITING;
        }
        if camera
            .compositing_space
            .is_some_and(|s| s == bevy::camera::CompositingSpace::Oklab)
        {
            view_key |= TextModeSpritePipelineKey::OKLAB_COMPOSITING;
        }

        if !camera.hdr {
            if let Some(tonemapping) = tonemapping {
                view_key |= TextModeSpritePipelineKey::TONEMAP_IN_SHADER;
                view_key |= match tonemapping {
                    Tonemapping::None => TextModeSpritePipelineKey::TONEMAP_METHOD_NONE,
                    Tonemapping::Reinhard => TextModeSpritePipelineKey::TONEMAP_METHOD_REINHARD,
                    Tonemapping::ReinhardLuminance => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_REINHARD_LUMINANCE
                    }
                    Tonemapping::AcesFitted => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_ACES_FITTED
                    }
                    Tonemapping::AgX => TextModeSpritePipelineKey::TONEMAP_METHOD_AGX,
                    Tonemapping::SomewhatBoringDisplayTransform => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM
                    }
                    Tonemapping::TonyMcMapface => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE
                    }
                    Tonemapping::BlenderFilmic => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_BLENDER_FILMIC
                    }
                    Tonemapping::KhronosPbrNeutral => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_PBR_NEUTRAL
                    }
                };
            }
            if let Some(DebandDither::Enabled) = dither {
                view_key |= TextModeSpritePipelineKey::DEBAND_DITHER;
            }
        }

        let pipeline = pipelines.specialize(&pipeline_cache, &sprite_pipeline, view_key);

        view_entities.clear();
        if let Some(visible_entities) = visible_entities.get::<TextModeSprite>() {
            view_entities.extend(
                visible_entities
                    .iter_visible()
                    .map(|(_, e)| e.index_u32() as usize),
            );
        }

        transparent_phase
            .items
            .reserve(extracted_sprites.sprites.len());

        for (index, extracted_sprite) in extracted_sprites.sprites.iter().enumerate() {
            let view_index = extracted_sprite.main_entity.index_u32();

            if !view_entities.contains(view_index as usize) {
                continue;
            }

            // These items will be sorted by depth with other phase items
            let sort_key = FloatOrd(extracted_sprite.transform.translation().z);

            // Add the item to the render phase
            transparent_phase.add_transient(Transparent2d {
                draw_function: draw_sprite_function,
                pipeline,
                entity: (
                    extracted_sprite.render_entity,
                    extracted_sprite.main_entity.into(),
                ),
                sort_key,
                // `batch_range` is calculated in `prepare_text_mode_sprite_image_bind_groups`
                batch_range: 0..0,
                extra_index: PhaseItemExtraIndex::None,
                extracted_index: index,
                indexed: true,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_text_mode_sprite_view_bind_groups(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    sprite_pipeline: Res<TextModeSpritePipeline>,
    view_uniforms: Res<ViewUniforms>,
    views: Query<(Entity, &Tonemapping), With<ExtractedView>>,
    tonemapping_luts: Res<TonemappingLuts>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>,
) {
    let Some(view_binding) = view_uniforms.uniforms.binding() else {
        return;
    };

    for (entity, tonemapping) in &views {
        let lut_bindings =
            get_lut_bindings(&images, &tonemapping_luts, tonemapping, &fallback_image);
        let view_bind_group = render_device.create_bind_group(
            "text_mode_sprite_view_bind_group",
            &pipeline_cache.get_bind_group_layout(&sprite_pipeline.view_layout),
            &BindGroupEntries::sequential((view_binding.clone(), lut_bindings.0, lut_bindings.1)),
        );

        commands.entity(entity).insert(TextModeSpriteViewBindGroup {
            value: view_bind_group,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_text_mode_sprite_image_bind_groups(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    pipeline_cache: Res<PipelineCache>,
    mut sprite_meta: ResMut<TextModeSpriteMeta>,
    sprite_pipeline: Res<TextModeSpritePipeline>,
    mut image_bind_groups: ResMut<TextModeImageBindGroups>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    extracted_sprites: Res<ExtractedTextModeSprites>,
    extracted_slices: Res<ExtractedTextModeSlices>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent2d>>,
    events: Res<TextModeSpriteAssetEvents>,
    mut batches: ResMut<TextModeSpriteBatches>,
) {
    // If an image has changed, the GpuImage has (probably) changed
    for event in &events.images {
        match event {
            AssetEvent::Added { .. } |
            // Images don't have dependencies
            AssetEvent::LoadedWithDependencies { .. } => {}
            AssetEvent::Unused { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                image_bind_groups.values.remove(id);
            }
        };
    }

    batches.clear();

    // Clear the sprite instances
    sprite_meta.sprite_instance_buffer.clear();

    // Index buffer indices
    let mut index = 0;

    let image_bind_groups = &mut *image_bind_groups;

    for (retained_view, transparent_phase) in phases.iter_mut() {
        let mut current_batch = None;
        let mut batch_item_index = 0;
        let mut batch_image_size = Vec2::ZERO;
        let mut batch_image_handle = None;

        // Iterate through the phase items and detect when successive sprites that can be batched.
        for item_index in 0..transparent_phase.items.len() {
            let item = &transparent_phase.items[item_index];

            let Some(extracted_sprite) = extracted_sprites
                .sprites
                .get(item.extracted_index)
                .filter(|extracted_sprite| extracted_sprite.render_entity == item.entity())
            else {
                // If there is a phase item that is not a text mode sprite, then we must start a new
                // batch to draw the other phase item(s) and to respect draw order. This can be
                // done by invalidating the batch_image_handle
                batch_image_handle = None;
                continue;
            };

            if batch_image_handle != Some(extracted_sprite.image_handle_id) {
                let Some(gpu_image) = gpu_images.get(extracted_sprite.image_handle_id) else {
                    continue;
                };

                batch_image_size = gpu_image.size_2d().as_vec2();
                let image_handle = extracted_sprite.image_handle_id;
                batch_image_handle = Some(image_handle);
                image_bind_groups
                    .values
                    .entry(image_handle)
                    .or_insert_with(|| {
                        render_device.create_bind_group(
                            "text_mode_sprite_material_bind_group",
                            &pipeline_cache.get_bind_group_layout(&sprite_pipeline.material_layout),
                            &BindGroupEntries::sequential((
                                &gpu_image.texture_view,
                                &gpu_image.sampler,
                            )),
                        )
                    });

                batch_item_index = item_index;
                current_batch = Some(batches.entry((*retained_view, item.entity())).insert(
                    TextModeSpriteBatch {
                        image_handle_id: image_handle,
                        range: index..index,
                    },
                ));
            }

            let rotation = extracted_sprite.rotation % 4;

            match extracted_sprite.kind {
                TextModeExtractedSpriteKind::Single {
                    anchor,
                    rect,
                    custom_size,
                } => {
                    // By default, the size of the quad is the size of the texture
                    let mut quad_size = batch_image_size;

                    // Calculate vertex data for this item
                    // If a rect is specified, adjust UVs and the size of the quad
                    let mut uv_offset_scale = if let Some(rect) = rect {
                        let rect_size = rect.size();
                        quad_size = rect_size;
                        Vec4::new(
                            rect.min.x / batch_image_size.x,
                            rect.max.y / batch_image_size.y,
                            rect_size.x / batch_image_size.x,
                            -rect_size.y / batch_image_size.y,
                        )
                    } else {
                        Vec4::new(0.0, 1.0, 1.0, -1.0)
                    };

                    if extracted_sprite.flip_x {
                        uv_offset_scale.x += uv_offset_scale.z;
                        uv_offset_scale.z *= -1.0;
                    }
                    if extracted_sprite.flip_y {
                        uv_offset_scale.y += uv_offset_scale.w;
                        uv_offset_scale.w *= -1.0;
                    }

                    // Override the size if a custom one is specified
                    quad_size = custom_size.unwrap_or(quad_size);

                    let translation = quad_size * (-anchor - Vec2::splat(0.5));
                    let scale = quad_size.extend(1.0);

                    let rotation_affine = rotation_affine(rotation, quad_size);

                    let transform = extracted_sprite.transform.affine()
                        * Affine3A::from_translation(translation.extend(0.0))
                        * rotation_affine
                        * Affine3A::from_scale(scale);

                    // Store the vertex data and add the item to the render phase
                    sprite_meta
                        .sprite_instance_buffer
                        .push(TextModeSpriteInstance::from(
                            &transform,
                            &extracted_sprite.bg,
                            &extracted_sprite.fg,
                            extracted_sprite.alpha,
                            &uv_offset_scale,
                        ));

                    current_batch.as_mut().unwrap().get_mut().range.end += 1;
                    index += 1;
                }
                TextModeExtractedSpriteKind::Slices { ref indices } => {
                    for i in indices.clone() {
                        let slice = &extracted_slices.slices[i];
                        let rect = slice.rect;
                        let rect_size = rect.size();

                        // Calculate vertex data for this item
                        let mut uv_offset_scale = Vec4::new(
                            rect.min.x / batch_image_size.x,
                            rect.max.y / batch_image_size.y,
                            rect_size.x / batch_image_size.x,
                            -rect_size.y / batch_image_size.y,
                        );

                        if extracted_sprite.flip_x {
                            uv_offset_scale.x += uv_offset_scale.z;
                            uv_offset_scale.z *= -1.0;
                        }
                        if extracted_sprite.flip_y {
                            uv_offset_scale.y += uv_offset_scale.w;
                            uv_offset_scale.w *= -1.0;
                        }

                        let rotation_affine = rotation_affine(rotation, slice.size);

                        let transform = extracted_sprite.transform.affine()
                            * Affine3A::from_translation(
                                (slice.size * -Vec2::splat(0.5) + slice.offset).extend(0.0),
                            )
                            * rotation_affine
                            * Affine3A::from_scale(slice.size.extend(1.0));

                        // Store the vertex data and add the item to the render phase
                        sprite_meta
                            .sprite_instance_buffer
                            .push(TextModeSpriteInstance::from(
                                &transform,
                                &extracted_sprite.bg,
                                &extracted_sprite.fg,
                                extracted_sprite.alpha,
                                &uv_offset_scale,
                            ));

                        current_batch.as_mut().unwrap().get_mut().range.end += 1;
                        index += 1;
                    }
                }
            }
            transparent_phase.items[batch_item_index]
                .batch_range_mut()
                .end += 1;
        }
        sprite_meta
            .sprite_instance_buffer
            .write_buffer(&render_device, &render_queue);

        if sprite_meta.sprite_index_buffer.len() != 6 {
            sprite_meta.sprite_index_buffer.clear();

            // NOTE: This code is creating 6 indices pointing to 4 vertices.
            // The vertices form the corners of a quad based on their two least significant bits.
            // 10   11
            //
            // 00   01
            // The sprite shader can then use the two least significant bits as the vertex index.
            // The rest of the properties to transform the vertex positions and UVs (which are
            // implicit) are baked into the instance transform, and UV offset and scale.
            sprite_meta.sprite_index_buffer.push(2);
            sprite_meta.sprite_index_buffer.push(0);
            sprite_meta.sprite_index_buffer.push(1);
            sprite_meta.sprite_index_buffer.push(1);
            sprite_meta.sprite_index_buffer.push(3);
            sprite_meta.sprite_index_buffer.push(2);

            sprite_meta
                .sprite_index_buffer
                .write_buffer(&render_device, &render_queue);
        }
    }
}

/// Builds the affine transform for `rotation` quarter turns around the center of a `quad_size` quad.
#[inline]
fn rotation_affine(rotation: u8, quad_size: Vec2) -> Affine3A {
    if rotation == 0 {
        Affine3A::IDENTITY
    } else {
        Affine3A::from_translation((quad_size * Vec2::new(0.5, 0.5)).extend(0.0))
            * Affine3A::from_rotation_z(PI / 2.0 * f32::from(rotation))
            * Affine3A::from_translation((quad_size * Vec2::new(-0.5, -0.5)).extend(0.0))
    }
}

pub type DrawTextModeSprite = (
    SetItemPipeline,
    SetTextModeSpriteViewBindGroup<0>,
    SetTextModeSpriteTextureBindGroup<1>,
    DrawTextModeSpriteBatch,
);

pub struct SetTextModeSpriteViewBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTextModeSpriteViewBindGroup<I> {
    type Param = ();
    type ViewQuery = (Read<ViewUniformOffset>, Read<TextModeSpriteViewBindGroup>);
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        (view_uniform, sprite_view_bind_group): ROQueryItem<'w, '_, Self::ViewQuery>,
        _entity: Option<()>,
        _param: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(I, &sprite_view_bind_group.value, &[view_uniform.offset]);
        RenderCommandResult::Success
    }
}

pub struct SetTextModeSpriteTextureBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTextModeSpriteTextureBindGroup<I> {
    type Param = (SRes<TextModeImageBindGroups>, SRes<TextModeSpriteBatches>);
    type ViewQuery = Read<ExtractedView>;
    type ItemQuery = ();

    fn render<'w>(
        item: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        _entity: Option<()>,
        (image_bind_groups, batches): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let image_bind_groups = image_bind_groups.into_inner();
        let Some(batch) = batches.get(&(view.retained_view_entity, item.entity())) else {
            return RenderCommandResult::Skip;
        };

        pass.set_bind_group(
            I,
            image_bind_groups
                .values
                .get(&batch.image_handle_id)
                .unwrap(),
            &[],
        );
        RenderCommandResult::Success
    }
}

pub struct DrawTextModeSpriteBatch;
impl<P: PhaseItem> RenderCommand<P> for DrawTextModeSpriteBatch {
    type Param = (SRes<TextModeSpriteMeta>, SRes<TextModeSpriteBatches>);
    type ViewQuery = Read<ExtractedView>;
    type ItemQuery = ();

    fn render<'w>(
        item: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        _entity: Option<()>,
        (sprite_meta, batches): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let sprite_meta = sprite_meta.into_inner();
        let Some(batch) = batches.get(&(view.retained_view_entity, item.entity())) else {
            return RenderCommandResult::Skip;
        };

        pass.set_index_buffer(
            sprite_meta.sprite_index_buffer.buffer().unwrap().slice(..),
            IndexFormat::Uint32,
        );
        pass.set_vertex_buffer(
            0,
            sprite_meta
                .sprite_instance_buffer
                .buffer()
                .unwrap()
                .slice(..),
        );
        pass.draw_indexed(0..6, 0, batch.range.clone());
        RenderCommandResult::Success
    }
}
