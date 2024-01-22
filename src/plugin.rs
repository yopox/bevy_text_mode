use std::ops::Range;
use bevy::asset::load_internal_asset;

use bevy::core::{Pod, Zeroable};
use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::ecs::system::{SystemParamItem, SystemState};
use bevy::ecs::system::lifetimeless::{Read, SRes};
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::{Extract, Render, RenderApp, RenderSet};
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::*;
use bevy::render::render_resource::{BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState, BufferBindingType, BufferUsages, BufferVec, ColorTargetState, ColorWrites, FragmentState, FrontFace, ImageCopyTexture, ImageDataLayout, IndexFormat, MultisampleState, Origin3d, PipelineCache, PolygonMode, PrimitiveState, RenderPipelineDescriptor, SamplerBindingType, ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines, TextureAspect, TextureFormat, TextureSampleType, TextureViewDescriptor, TextureViewDimension, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::{BevyDefault, DefaultImageSampler, GpuImage, ImageSampler, TextureFormatPixelInfo};
use bevy::render::view::{ExtractedView, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms, VisibleEntities};
use bevy::sprite::{queue_material2d_meshes, SpriteAssetEvents, SpriteSystem};
use bevy::utils::{EntityHashMap, FloatOrd, HashMap};
use fixedbitset::FixedBitSet;

use crate::text_mode_texture_atlas::TextModeTextureAtlasSprite;

const SPRITE_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(1354325909327402345);

pub struct TextModePlugin;

impl Plugin for TextModePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SPRITE_SHADER_HANDLE,
            "text_mode_sprite.wgsl",
            Shader::from_wgsl
        );

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<TextModeImageBindGroups>()
                .init_resource::<SpecializedRenderPipelines<TextModeSpritePipeline>>()
                .init_resource::<TextModeSpriteMeta>()
                .init_resource::<ExtractedTextModeSprites>()
                .init_resource::<TextModeSpriteAssetEvents>()
                .add_render_command::<Transparent2d, DrawTextModeSprite>()
                .add_systems(
                    ExtractSchedule,
                    (
                        extract_sprites.in_set(SpriteSystem::ExtractSprites),
                        extract_sprite_events,
                    ),
                )
                .add_systems(
                    Render,
                    (
                        queue_sprites
                            .in_set(RenderSet::Queue)
                            .ambiguous_with(queue_material2d_meshes::<ColorMaterial>),
                        prepare_sprites.in_set(RenderSet::PrepareBindGroups),
                    ),
                )
            ;
        };
    }

    fn finish(&self, app: &mut App) {
        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<TextModeSpritePipeline>();
        }
    }
}

#[derive(Resource)]
pub struct TextModeSpritePipeline {
    view_layout: BindGroupLayout,
    material_layout: BindGroupLayout,
    pub dummy_white_gpu_image: GpuImage,
}

impl FromWorld for TextModeSpritePipeline {
    fn from_world(world: &mut World) -> Self {
        let mut system_state: SystemState<(
            Res<RenderDevice>,
            Res<DefaultImageSampler>,
            Res<RenderQueue>,
        )> = SystemState::new(world);
        let (render_device, default_sampler, render_queue) = system_state.get_mut(world);

        let view_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(ViewUniform::min_size()),
                },
                count: None,
            }],
            label: Some("text_mode_sprite_view_layout"),
        });

        let material_layout = render_device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some("text_mode_sprite_material_layout"),
        });
        let dummy_white_gpu_image = {
            let image = Image::default();
            let texture = render_device.create_texture(&image.texture_descriptor);
            let sampler = match image.sampler {
                ImageSampler::Default => (**default_sampler).clone(),
                ImageSampler::Descriptor(ref descriptor) => {
                    render_device.create_sampler(&descriptor.as_wgpu())
                }
            };

            let format_size = image.texture_descriptor.format.pixel_size();
            render_queue.write_texture(
                ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &image.data,
                ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width() * format_size as u32),
                    rows_per_image: None,
                },
                image.texture_descriptor.size,
            );
            let texture_view = texture.create_view(&TextureViewDescriptor::default());
            GpuImage {
                texture,
                texture_view,
                texture_format: image.texture_descriptor.format,
                sampler,
                size: image.size_f32(),
                mip_level_count: image.texture_descriptor.mip_level_count,
            }
        };

        TextModeSpritePipeline {
            view_layout,
            material_layout,
            dummy_white_gpu_image,
        }
    }
}


bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct TextModeSpritePipelineKey: u32 {
        const NONE                              = 0;
        const COLORED                           = 1 << 0;
        const HDR                               = 1 << 1;
        const TONEMAP_IN_SHADER                 = 1 << 2;
        const DEBAND_DITHER                     = 1 << 3;
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
    }
}

impl TextModeSpritePipelineKey {
    const MSAA_MASK_BITS: u32 = 0b111;
    const MSAA_SHIFT_BITS: u32 = 32 - Self::MSAA_MASK_BITS.count_ones();
    const TONEMAP_METHOD_MASK_BITS: u32 = 0b111;
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
    pub const fn from_hdr(hdr: bool) -> Self {
        if hdr {
            TextModeSpritePipelineKey::HDR
        } else {
            TextModeSpritePipelineKey::NONE
        }
    }

    #[inline]
    pub const fn from_colored(colored: bool) -> Self {
        if colored {
            TextModeSpritePipelineKey::COLORED
        } else {
            TextModeSpritePipelineKey::NONE
        }
    }
}

impl SpecializedRenderPipeline for TextModeSpritePipeline {
    type Key = TextModeSpritePipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let formats = vec![
            VertexFormat::Float32x4, // position
            VertexFormat::Float32x4, // position
            VertexFormat::Float32x4, // position
            VertexFormat::Float32x4, // bg
            VertexFormat::Float32x4, // fg
            VertexFormat::Float32, // alpha
            VertexFormat::Float32x4, // uv
            VertexFormat::Float32x3, // padding
        ];

        let vertex_layout = VertexBufferLayout::from_vertex_formats(VertexStepMode::Vertex, formats);

        let mut shader_defs = Vec::new();

        if key.contains(TextModeSpritePipelineKey::TONEMAP_IN_SHADER) {
            shader_defs.push("TONEMAP_IN_SHADER".into());

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
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM
            {
                shader_defs.push("TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_BLENDER_FILMIC {
                shader_defs.push("TONEMAP_METHOD_BLENDER_FILMIC".into());
            } else if method == TextModeSpritePipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE {
                shader_defs.push("TONEMAP_METHOD_TONY_MC_MAPFACE".into());
            }

            // Debanding is tied to tonemapping in the shader, cannot run without it.
            if key.contains(TextModeSpritePipelineKey::DEBAND_DITHER) {
                shader_defs.push("DEBAND_DITHER".into());
            }
        }

        let format = match key.contains(TextModeSpritePipelineKey::HDR) {
            true => ViewTarget::TEXTURE_FORMAT_HDR,
            false => TextureFormat::bevy_default(),
        };

        RenderPipelineDescriptor {
            vertex: VertexState {
                shader: SPRITE_SHADER_HANDLE,
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![vertex_layout],
            },
            fragment: Some(FragmentState {
                shader: SPRITE_SHADER_HANDLE,
                shader_defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            layout: vec![self.view_layout.clone(), self.material_layout.clone()],
            primitive: PrimitiveState {
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            label: Some("text_mode_sprite_pipeline".into()),
            push_constant_ranges: Vec::new(),
        }
    }
}

/// See [bevy::sprite::ExtractedSprite]
#[derive(Component, Clone, Copy)]
pub struct ExtractedTextModeSprite {
    pub transform: GlobalTransform,
    pub bg: Color,
    pub fg: Color,
    pub alpha: f32,
    pub rect: Option<Rect>,
    pub custom_size: Option<Vec2>,
    pub image_handle_id: AssetId<Image>,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
    pub anchor: Vec2,
    pub original_entity: Option<Entity>,
}

#[derive(Resource, Default)]
pub struct ExtractedTextModeSprites {
    pub sprites: EntityHashMap<Entity, ExtractedTextModeSprite>,
}

#[derive(Resource, Default)]
pub struct TextModeSpriteAssetEvents {
    pub images: Vec<AssetEvent<Image>>,
}

pub fn extract_sprite_events(
    mut events: ResMut<TextModeSpriteAssetEvents>,
    mut image_events: Extract<EventReader<AssetEvent<Image>>>,
) {
    let TextModeSpriteAssetEvents { ref mut images } = *events;
    images.clear();

    for event in image_events.read() {
        images.push(*event);
    }
}

/// See [bevy::sprite::extract_sprites]
pub fn extract_sprites(
    mut extracted_sprites: ResMut<ExtractedTextModeSprites>,
    texture_atlases: Extract<Res<Assets<TextureAtlas>>>,
    atlas_query: Extract<
        Query<(
            Entity,
            &ViewVisibility,
            &TextModeTextureAtlasSprite,
            &GlobalTransform,
            &Handle<TextureAtlas>,
        )>,
    >,
) {
    extracted_sprites.sprites.clear();

    for (entity, visibility, atlas_sprite, transform, texture_atlas_handle) in atlas_query.iter() {
        if !visibility.get() {
            continue;
        }
        if let Some(texture_atlas) = texture_atlases.get(texture_atlas_handle) {
            let rect = Some(
                *texture_atlas
                    .textures
                    .get(atlas_sprite.index)
                    .unwrap_or_else(|| {
                        panic!(
                            "Sprite index {:?} does not exist for texture atlas handle {:?}.",
                            atlas_sprite.index,
                            texture_atlas_handle.id(),
                        )
                    }),
            );
            extracted_sprites.sprites.insert(entity, ExtractedTextModeSprite {
                bg: atlas_sprite.bg,
                fg: atlas_sprite.fg,
                alpha: atlas_sprite.alpha,
                transform: *transform,
                // Select the area in the texture atlas
                rect,
                // Pass the custom size
                custom_size: atlas_sprite.custom_size,
                flip_x: atlas_sprite.flip_x,
                flip_y: atlas_sprite.flip_y,
                rotation: atlas_sprite.rotation,
                image_handle_id: texture_atlas.texture.id(),
                anchor: atlas_sprite.anchor.as_vec(),
                original_entity: None,
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
    fn from(transform: &Affine3A, bg: &Color, fg: &Color, alpha: f32, uv_offset_scale: &Vec4) -> Self {
        let transpose_model_3x3 = transform.matrix3.transpose();
        Self {
            i_model_transpose: [
                transpose_model_3x3.x_axis.extend(transform.translation.x),
                transpose_model_3x3.y_axis.extend(transform.translation.y),
                transpose_model_3x3.z_axis.extend(transform.translation.z),
            ],
            i_bg: bg.as_linear_rgba_f32(),
            i_fg: fg.as_linear_rgba_f32(),
            i_alpha: alpha,
            i_uv: uv_offset_scale.to_array(),
            i_pad: [0., 0., 0.],
        }
    }
}

/// See [bevy::sprite::SpriteMeta]
#[derive(Resource)]
pub struct TextModeSpriteMeta {
    view_bind_group: Option<BindGroup>,
    sprite_index_buffer: BufferVec<u32>,
    sprite_instance_buffer: BufferVec<TextModeSpriteInstance>,
}

impl Default for TextModeSpriteMeta {
    fn default() -> Self {
        Self {
            view_bind_group: None,
            sprite_index_buffer: BufferVec::<u32>::new(BufferUsages::INDEX),
            sprite_instance_buffer: BufferVec::<TextModeSpriteInstance>::new(BufferUsages::VERTEX),
        }
    }
}

#[derive(Component, PartialEq, Eq, Clone)]
pub struct TextModeSpriteBatch {
    image_handle_id: AssetId<Image>,
    range: Range<u32>,
}

#[derive(Resource, Default)]
pub struct TextModeImageBindGroups {
    values: HashMap<AssetId<Image>, BindGroup>,
}

const QUAD_INDICES: [usize; 6] = [0, 2, 3, 0, 1, 2];

const QUAD_VERTEX_POSITIONS: [Vec2; 4] = [
    Vec2::new(-0.5, -0.5),
    Vec2::new(0.5, -0.5),
    Vec2::new(0.5, 0.5),
    Vec2::new(-0.5, 0.5),
];

const QUAD_UVS: [Vec2; 4] = [
    Vec2::new(0., 1.),
    Vec2::new(1., 1.),
    Vec2::new(1., 0.),
    Vec2::new(0., 0.),
];

/// See [bevy::sprite::queue_sprites]
#[allow(clippy::too_many_arguments)]
pub fn queue_sprites(
    mut view_entities: Local<FixedBitSet>,
    draw_functions: Res<DrawFunctions<Transparent2d>>,
    sprite_pipeline: Res<TextModeSpritePipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<TextModeSpritePipeline>>,
    pipeline_cache: Res<PipelineCache>,
    msaa: Res<Msaa>,
    extracted_sprites: Res<ExtractedTextModeSprites>,
    mut views: Query<(
        &mut RenderPhase<Transparent2d>,
        &VisibleEntities,
        &ExtractedView,
        Option<&Tonemapping>,
        Option<&DebandDither>,
    )>,
) {
    let msaa_key = TextModeSpritePipelineKey::from_msaa_samples(msaa.samples());

    let draw_sprite_function = draw_functions.read().id::<DrawTextModeSprite>();

    for (mut transparent_phase, visible_entities, view, tonemapping, dither) in &mut views {
        let mut view_key = TextModeSpritePipelineKey::from_hdr(view.hdr) | msaa_key;

        if !view.hdr {
            if let Some(tonemapping) = tonemapping {
                view_key |= TextModeSpritePipelineKey::TONEMAP_IN_SHADER;
                view_key |= match tonemapping {
                    Tonemapping::None => TextModeSpritePipelineKey::TONEMAP_METHOD_NONE,
                    Tonemapping::Reinhard => TextModeSpritePipelineKey::TONEMAP_METHOD_REINHARD,
                    Tonemapping::ReinhardLuminance => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_REINHARD_LUMINANCE
                    }
                    Tonemapping::AcesFitted => TextModeSpritePipelineKey::TONEMAP_METHOD_ACES_FITTED,
                    Tonemapping::AgX => TextModeSpritePipelineKey::TONEMAP_METHOD_AGX,
                    Tonemapping::SomewhatBoringDisplayTransform => {
                        TextModeSpritePipelineKey::TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM
                    }
                    Tonemapping::TonyMcMapface => TextModeSpritePipelineKey::TONEMAP_METHOD_TONY_MC_MAPFACE,
                    Tonemapping::BlenderFilmic => TextModeSpritePipelineKey::TONEMAP_METHOD_BLENDER_FILMIC,
                };
            }
            if let Some(DebandDither::Enabled) = dither {
                view_key |= TextModeSpritePipelineKey::DEBAND_DITHER;
            }
        }

        let pipeline = pipelines.specialize(
            &pipeline_cache,
            &sprite_pipeline,
            view_key | TextModeSpritePipelineKey::from_colored(false),
        );

        view_entities.clear();
        view_entities.extend(visible_entities.entities.iter().map(|e| e.index() as usize));

        transparent_phase
            .items
            .reserve(extracted_sprites.sprites.len());

        for (entity, extracted_sprite) in extracted_sprites.sprites.iter() {
            let index = extracted_sprite.original_entity.unwrap_or(*entity).index();

            if !view_entities.contains(index as usize) {
                continue;
            }

            // These items will be sorted by depth with other phase items
            let sort_key = FloatOrd(extracted_sprite.transform.translation().z);

            // Add the item to the render phase
            transparent_phase.add(Transparent2d {
                draw_function: draw_sprite_function,
                pipeline,
                entity: *entity,
                sort_key,
                // batch_range and dynamic_offset will be calculated in prepare_sprites
                batch_range: 0..0,
                dynamic_offset: None,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_sprites(
    mut commands: Commands,
    mut previous_len: Local<usize>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut sprite_meta: ResMut<TextModeSpriteMeta>,
    view_uniforms: Res<ViewUniforms>,
    sprite_pipeline: Res<TextModeSpritePipeline>,
    mut image_bind_groups: ResMut<TextModeImageBindGroups>,
    gpu_images: Res<RenderAssets<Image>>,
    extracted_sprites: Res<ExtractedTextModeSprites>,
    mut phases: Query<&mut RenderPhase<Transparent2d>>,
    events: Res<SpriteAssetEvents>,
) {
    // If an image has changed, the GpuImage has (probably) changed
    for event in &events.images {
        match event {
            AssetEvent::Added {..} |
            // images don't have dependencies
            AssetEvent::LoadedWithDependencies { .. } => {}
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                image_bind_groups.values.remove(id);
            }
        };
    }

    if let Some(view_binding) = view_uniforms.uniforms.binding() {
        let mut batches: Vec<(Entity, TextModeSpriteBatch)> = Vec::with_capacity(*previous_len);

        // Clear the sprite instances
        sprite_meta.sprite_instance_buffer.clear();

        sprite_meta.view_bind_group = Some(render_device.create_bind_group(
            "text_mode_sprite_view_bind_group",
            &sprite_pipeline.view_layout,
            &BindGroupEntries::single(view_binding),
        ));

        // Index buffer indices
        let mut index = 0;

        let image_bind_groups = &mut *image_bind_groups;

        for mut transparent_phase in &mut phases {
            let mut batch_item_index = 0;
            let mut batch_image_size = Vec2::ZERO;
            let mut batch_image_handle = AssetId::invalid();

            for item_index in 0..transparent_phase.items.len() {
                let item = &transparent_phase.items[item_index];
                let Some(extracted_sprite) = extracted_sprites.sprites.get(&item.entity) else {
                    batch_image_handle = AssetId::invalid();
                    continue;
                };

                let batch_image_changed = batch_image_handle != extracted_sprite.image_handle_id;
                if batch_image_changed {
                    let Some(gpu_image) = gpu_images.get(extracted_sprite.image_handle_id) else {
                        continue;
                    };

                    batch_image_size = Vec2::new(gpu_image.size.x, gpu_image.size.y);
                    batch_image_handle = extracted_sprite.image_handle_id;
                    image_bind_groups
                        .values
                        .entry(batch_image_handle)
                        .or_insert_with(|| {
                            render_device.create_bind_group(
                                "text_mode_sprite_material_bind_group",
                                &sprite_pipeline.material_layout,
                                &BindGroupEntries::sequential((
                                    &gpu_image.texture_view,
                                    &gpu_image.sampler,
                                )),
                            )
                        });
                }

                // By default, the size of the quad is the size of the texture
                let mut quad_size = batch_image_size;

                // Calculate vertex data for this item
                let mut uv_offset_scale: Vec4;

                // If a rect is specified, adjust UVs and the size of the quad
                if let Some(rect) = extracted_sprite.rect {
                    let rect_size = rect.size();
                    uv_offset_scale = Vec4::new(
                        rect.min.x / batch_image_size.x,
                        rect.max.y / batch_image_size.y,
                        rect_size.x / batch_image_size.x,
                        -rect_size.y / batch_image_size.y,
                    );
                    quad_size = rect_size;
                } else {
                    uv_offset_scale = Vec4::new(0.0, 1.0, 1.0, -1.0);
                }

                // TODO: Restore rotation
                // let mut uvs = QUAD_UVS;
                // uvs = match extracted_sprite.rotation % 4 {
                //     1 => [uvs[1], uvs[2], uvs[3], uvs[0]],
                //     2 => [uvs[2], uvs[3], uvs[0], uvs[1]],
                //     3 => [uvs[3], uvs[0], uvs[1], uvs[2]],
                //     _ => uvs
                // };

                if extracted_sprite.flip_x {
                    uv_offset_scale.x += uv_offset_scale.z;
                    uv_offset_scale.z *= -1.0;
                }
                if extracted_sprite.flip_y {
                    uv_offset_scale.y += uv_offset_scale.w;
                    uv_offset_scale.w *= -1.0;
                }

                // Override the size if a custom one is specified
                if let Some(custom_size) = extracted_sprite.custom_size {
                    quad_size = custom_size;
                }
                let transform = extracted_sprite.transform.affine()
                    * Affine3A::from_scale_rotation_translation(
                    quad_size.extend(1.0),
                    Quat::IDENTITY,
                    (quad_size * (-extracted_sprite.anchor - Vec2::splat(0.5))).extend(0.0),
                );

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

                if batch_image_changed {
                    batch_item_index = item_index;

                    batches.push((
                        item.entity,
                        TextModeSpriteBatch {
                            image_handle_id: batch_image_handle,
                            range: index..index,
                        },
                    ));
                }

                transparent_phase.items[batch_item_index]
                    .batch_range_mut()
                    .end += 1;
                batches.last_mut().unwrap().1.range.end += 1;
                index += 1;
            }
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
            // See bevy_sprite/src/render/sprite.wgsl for the details.
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

        *previous_len = batches.len();
        commands.insert_or_spawn_batch(batches);
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
    type Param = SRes<TextModeSpriteMeta>;
    type ViewWorldQuery = Read<ViewUniformOffset>;
    type ItemWorldQuery = ();

    fn render<'w>(
        _item: &P,
        view_uniform: &'_ ViewUniformOffset,
        _entity: (),
        sprite_meta: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(
            I,
            sprite_meta.into_inner().view_bind_group.as_ref().unwrap(),
            &[view_uniform.offset],
        );
        RenderCommandResult::Success
    }
}
pub struct SetTextModeSpriteTextureBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTextModeSpriteTextureBindGroup<I> {
    type Param = SRes<TextModeImageBindGroups>;
    type ViewWorldQuery = ();
    type ItemWorldQuery = Read<TextModeSpriteBatch>;

    fn render<'w>(
        _item: &P,
        _view: (),
        batch: &'_ TextModeSpriteBatch,
        image_bind_groups: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let image_bind_groups = image_bind_groups.into_inner();

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
    type Param = SRes<TextModeSpriteMeta>;
    type ViewWorldQuery = ();
    type ItemWorldQuery = Read<TextModeSpriteBatch>;

    fn render<'w>(
        _item: &P,
        _view: (),
        batch: &'_ TextModeSpriteBatch,
        sprite_meta: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let sprite_meta = sprite_meta.into_inner();
        pass.set_index_buffer(
            sprite_meta.sprite_index_buffer.buffer().unwrap().slice(..),
            0,
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