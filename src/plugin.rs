use std::cmp::Ordering;
use bevy::asset::HandleId;
use bevy::core::{Pod, Zeroable};
use bevy::core_pipeline::core_2d::Transparent2d;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::lifetimeless::{Read, SQuery, SRes};
use bevy::ecs::system::{SystemParamItem, SystemState};
use bevy::prelude::*;
use bevy::reflect::TypeUuid;
use bevy::render::{Extract, RenderApp, RenderStage};
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_phase::*;
use bevy::render::render_resource::{BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendState, BufferBindingType, BufferUsages, BufferVec, ColorTargetState, ColorWrites, Extent3d, FragmentState, FrontFace, ImageCopyTexture, ImageDataLayout, MultisampleState, Origin3d, PipelineCache, PolygonMode, PrimitiveState, RenderPipelineDescriptor, SamplerBindingType, ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines, TextureAspect, TextureDimension, TextureFormat, TextureSampleType, TextureViewDescriptor, TextureViewDimension, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::{BevyDefault, DefaultImageSampler, GpuImage, ImageSampler, TextureFormatPixelInfo};
use bevy::render::view::{ExtractedView, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms, VisibleEntities};
use bevy::sprite::SpriteAssetEvents;
use bevy::utils::{FloatOrd, HashMap, Uuid};
use fixedbitset::FixedBitSet;
use crate::text_mode_texture_atlas::TextModeTextureAtlasSprite;

const SPRITE_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 1354325909327402345);

pub struct TextModePlugin;

impl Plugin for TextModePlugin {
    fn build(&self, app: &mut App) {
        let mut shaders = app.world.resource_mut::<Assets<Shader>>();
        let sprite_shader = Shader::from_wgsl(include_str!("text_mode_sprite.wgsl"));
        shaders.set_untracked(SPRITE_SHADER_HANDLE, sprite_shader);

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<TextModeSpritePipeline>()
                .init_resource::<SpecializedRenderPipelines<TextModeSpritePipeline>>()
                .init_resource::<TextModeImageBindGroups>()
                .init_resource::<TextModeSpriteMeta>()
                .init_resource::<ExtractedTextModeSprites>()
                .add_render_command::<Transparent2d, DrawTextModeSprite>()
                .add_system_to_stage(RenderStage::Extract, extract_sprites)
                .add_system_to_stage(RenderStage::Queue, queue_sprites)
            ;
        };
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
            let image = Image::new_fill(
                Extent3d::default(),
                TextureDimension::D2,
                &[255u8; 4],
                TextureFormat::bevy_default(),
            );
            let texture = render_device.create_texture(&image.texture_descriptor);
            let sampler = match image.sampler_descriptor {
                ImageSampler::Default => (**default_sampler).clone(),
                ImageSampler::Descriptor(descriptor) => render_device.create_sampler(&descriptor),
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
                    bytes_per_row: Some(
                        std::num::NonZeroU32::new(
                            image.texture_descriptor.size.width * format_size as u32,
                        )
                            .unwrap(),
                    ),
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
                size: Vec2::new(
                    image.texture_descriptor.size.width as f32,
                    image.texture_descriptor.size.height as f32,
                ),
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
    #[repr(transparent)]
    // NOTE: Apparently quadro drivers support up to 64x MSAA.
    // MSAA uses the highest 3 bits for the MSAA log2(sample count) to support up to 128x MSAA.
    pub struct TextModeSpritePipelineKey: u32 {
        const NONE                        = 0;
        const COLORED                     = (1 << 0);
        const HDR                         = (1 << 1);
        const TONEMAP_IN_SHADER           = (1 << 2);
        const DEBAND_DITHER               = (1 << 3);
        const MSAA_RESERVED_BITS          = Self::MSAA_MASK_BITS << Self::MSAA_SHIFT_BITS;
    }
}

impl TextModeSpritePipelineKey {
    const MSAA_MASK_BITS: u32 = 0b111;
    const MSAA_SHIFT_BITS: u32 = 32 - Self::MSAA_MASK_BITS.count_ones();

    pub fn from_msaa_samples(msaa_samples: u32) -> Self {
        let msaa_bits =
            (msaa_samples.trailing_zeros() & Self::MSAA_MASK_BITS) << Self::MSAA_SHIFT_BITS;
        Self::from_bits(msaa_bits).unwrap()
    }

    pub fn msaa_samples(&self) -> u32 {
        1 << ((self.bits >> Self::MSAA_SHIFT_BITS) & Self::MSAA_MASK_BITS)
    }

    pub fn from_hdr(hdr: bool) -> Self {
        if hdr {
            TextModeSpritePipelineKey::HDR
        } else {
            TextModeSpritePipelineKey::NONE
        }
    }
}

impl SpecializedRenderPipeline for TextModeSpritePipeline {
    type Key = TextModeSpritePipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let formats = vec![
            // position
            VertexFormat::Float32x3,
            // uv
            VertexFormat::Float32x2,
            // bg
            VertexFormat::Float32x4,
            // fg
            VertexFormat::Float32x4,
        ];

        let vertex_layout =
            VertexBufferLayout::from_vertex_formats(VertexStepMode::Vertex, formats);

        let mut shader_defs = Vec::new();

        if key.contains(TextModeSpritePipelineKey::TONEMAP_IN_SHADER) {
            shader_defs.push("TONEMAP_IN_SHADER".to_string());

            // Debanding is tied to tonemapping in the shader, cannot run without it.
            if key.contains(TextModeSpritePipelineKey::DEBAND_DITHER) {
                shader_defs.push("DEBAND_DITHER".to_string());
            }
        }

        let format = match key.contains(TextModeSpritePipelineKey::HDR) {
            true => ViewTarget::TEXTURE_FORMAT_HDR,
            false => TextureFormat::bevy_default(),
        };

        RenderPipelineDescriptor {
            vertex: VertexState {
                shader: SPRITE_SHADER_HANDLE.typed::<Shader>(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![vertex_layout],
            },
            fragment: Some(FragmentState {
                shader: SPRITE_SHADER_HANDLE.typed::<Shader>(),
                shader_defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            layout: Some(vec![self.view_layout.clone(), self.material_layout.clone()]),
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
        }
    }
}

/// See [bevy::sprite::SpriteBatch]
#[derive(Component, Eq, PartialEq, Copy, Clone)]
pub struct TextModeSpriteBatch {
    image_handle_id: HandleId,
}

/// See [bevy::sprite::ExtractedSprite]
#[derive(Component, Clone, Copy)]
pub struct ExtractedTextModeSprite {
    pub entity: Entity,
    pub transform: GlobalTransform,
    pub bg: Color,
    pub fg: Color,
    pub rect: Option<Rect>,
    pub custom_size: Option<Vec2>,
    pub image_handle_id: HandleId,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
    pub anchor: Vec2,
}

#[derive(Resource, Default)]
pub struct ExtractedTextModeSprites {
    pub sprites: Vec<ExtractedTextModeSprite>,
}

/// See [bevy::sprite::extract_sprites]
pub fn extract_sprites(
    mut extracted_sprites: ResMut<ExtractedTextModeSprites>,
    texture_atlases: Extract<Res<Assets<TextureAtlas>>>,
    atlas_query: Extract<
        Query<(
            Entity,
            &ComputedVisibility,
            &TextModeTextureAtlasSprite,
            &GlobalTransform,
            &Handle<TextureAtlas>,
        )>,
    >,
) {
    extracted_sprites.sprites.clear();
    for (entity, visibility, atlas_sprite, transform, texture_atlas_handle) in atlas_query.iter() {
        if !visibility.is_visible() {
            continue;
        }
        if let Some(texture_atlas) = texture_atlases.get(texture_atlas_handle) {
            let rect = Some(texture_atlas.textures[atlas_sprite.index]);
            extracted_sprites.sprites.push(ExtractedTextModeSprite {
                entity,
                bg: atlas_sprite.bg,
                fg: atlas_sprite.fg,
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
            });
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct TextModeSpriteVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub bg: [f32; 4],
    pub fg: [f32; 4],
}


/// See [bevy::sprite::SpriteMeta]
#[derive(Resource)]
pub struct TextModeSpriteMeta {
    vertices: BufferVec<TextModeSpriteVertex>,
    view_bind_group: Option<BindGroup>,
}

impl Default for TextModeSpriteMeta {
    fn default() -> Self {
        Self {
            vertices: BufferVec::new(BufferUsages::VERTEX),
            view_bind_group: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct TextModeImageBindGroups {
    values: HashMap<Handle<Image>, BindGroup>,
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
    mut commands: Commands,
    mut view_entities: Local<FixedBitSet>,
    draw_functions: Res<DrawFunctions<Transparent2d>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut sprite_meta: ResMut<TextModeSpriteMeta>,
    view_uniforms: Res<ViewUniforms>,
    sprite_pipeline: Res<TextModeSpritePipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<TextModeSpritePipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut image_bind_groups: ResMut<TextModeImageBindGroups>,
    gpu_images: Res<RenderAssets<Image>>,
    msaa: Res<Msaa>,
    mut extracted_sprites: ResMut<ExtractedTextModeSprites>,
    mut views: Query<(
        &mut RenderPhase<Transparent2d>,
        &VisibleEntities,
        &ExtractedView,
        Option<&Tonemapping>,
    )>,
    events: Res<SpriteAssetEvents>,
) {
    // If an image has changed, the GpuImage has (probably) changed
    for event in &events.images {
        match event {
            AssetEvent::Created { .. } => None,
            AssetEvent::Modified { handle } | AssetEvent::Removed { handle } => {
                image_bind_groups.values.remove(handle)
            }
        };
    }

    let msaa_key = TextModeSpritePipelineKey::from_msaa_samples(msaa.samples);

    if let Some(view_binding) = view_uniforms.uniforms.binding() {
        let sprite_meta = &mut sprite_meta;

        // Clear the vertex buffers
        sprite_meta.vertices.clear();

        sprite_meta.view_bind_group = Some(render_device.create_bind_group(&BindGroupDescriptor {
            entries: &[BindGroupEntry {
                binding: 0,
                resource: view_binding,
            }],
            label: Some("text_mode_sprite_view_bind_group"),
            layout: &sprite_pipeline.view_layout,
        }));

        let draw_sprite_function = draw_functions.read().get_id::<DrawTextModeSprite>().unwrap();

        // Vertex buffer indices
        let mut index = 0;

        let extracted_sprites = &mut extracted_sprites.sprites;
        // Sort sprites by z for correct transparency and then by handle to improve batching
        // NOTE: This can be done independent of views by reasonably assuming that all 2D views look along the negative-z axis in world space
        extracted_sprites.sort_unstable_by(|a, b| {
            match a
                .transform
                .translation()
                .z
                .partial_cmp(&b.transform.translation().z)
            {
                Some(Ordering::Equal) | None => a.image_handle_id.cmp(&b.image_handle_id),
                Some(other) => other,
            }
        });
        let image_bind_groups = &mut *image_bind_groups;

        for (mut transparent_phase, visible_entities, view, tonemapping) in &mut views {
            let mut view_key = TextModeSpritePipelineKey::from_hdr(view.hdr) | msaa_key;
            if let Some(Tonemapping::Enabled { deband_dither }) = tonemapping {
                if !view.hdr {
                    view_key |= TextModeSpritePipelineKey::TONEMAP_IN_SHADER;

                    if *deband_dither {
                        view_key |= TextModeSpritePipelineKey::DEBAND_DITHER;
                    }
                }
            }
            let pipeline = pipelines.specialize(
                &mut pipeline_cache,
                &sprite_pipeline,
                view_key,
            );

            view_entities.clear();
            view_entities.extend(visible_entities.entities.iter().map(|e| e.index() as usize));
            transparent_phase.items.reserve(extracted_sprites.len());

            // Impossible starting values that will be replaced on the first iteration
            let mut current_batch = TextModeSpriteBatch {
                image_handle_id: HandleId::Id(Uuid::nil(), u64::MAX),
            };
            let mut current_batch_entity = Entity::from_raw(u32::MAX);
            let mut current_image_size = Vec2::ZERO;
            // Add a phase item for each sprite, and detect when successive items can be batched.
            // Spawn an entity with a `SpriteBatch` component for each possible batch.
            // Compatible items share the same entity.
            // Batches are merged later (in `batch_phase_system()`), so that they can be interrupted
            // by any other phase item (and they can interrupt other items from batching).
            for extracted_sprite in extracted_sprites.iter() {
                if !view_entities.contains(extracted_sprite.entity.index() as usize) {
                    continue;
                }
                let new_batch = TextModeSpriteBatch {
                    image_handle_id: extracted_sprite.image_handle_id,
                };
                if new_batch != current_batch {
                    // Set-up a new possible batch
                    if let Some(gpu_image) =
                        gpu_images.get(&Handle::weak(new_batch.image_handle_id))
                    {
                        current_batch = new_batch;
                        current_image_size = Vec2::new(gpu_image.size.x, gpu_image.size.y);
                        current_batch_entity = commands.spawn(current_batch).id();

                        image_bind_groups
                            .values
                            .entry(Handle::weak(current_batch.image_handle_id))
                            .or_insert_with(|| {
                                render_device.create_bind_group(&BindGroupDescriptor {
                                    entries: &[
                                        BindGroupEntry {
                                            binding: 0,
                                            resource: BindingResource::TextureView(
                                                &gpu_image.texture_view,
                                            ),
                                        },
                                        BindGroupEntry {
                                            binding: 1,
                                            resource: BindingResource::Sampler(&gpu_image.sampler),
                                        },
                                    ],
                                    label: Some("text_mode_sprite_material_bind_group"),
                                    layout: &sprite_pipeline.material_layout,
                                })
                            });
                    } else {
                        // Skip this item if the texture is not ready
                        continue;
                    }
                }

                // Calculate vertex data for this item

                let mut uvs = QUAD_UVS;

                uvs = match extracted_sprite.rotation % 4 {
                    1 => [uvs[1], uvs[2], uvs[3], uvs[0]],
                    2 => [uvs[2], uvs[3], uvs[0], uvs[1]],
                    3 => [uvs[3], uvs[0], uvs[1], uvs[2]],
                    _ => uvs
                };

                if extracted_sprite.flip_x {
                    uvs = [uvs[1], uvs[0], uvs[3], uvs[2]];
                }
                if extracted_sprite.flip_y {
                    uvs = [uvs[3], uvs[2], uvs[1], uvs[0]];
                }


                // By default, the size of the quad is the size of the texture
                let mut quad_size = current_image_size;

                // If a rect is specified, adjust UVs and the size of the quad
                if let Some(rect) = extracted_sprite.rect {
                    let rect_size = rect.size();
                    for uv in &mut uvs {
                        *uv = (rect.min + *uv * rect_size) / current_image_size;
                    }
                    quad_size = rect_size;
                }

                // Override the size if a custom one is specified
                if let Some(custom_size) = extracted_sprite.custom_size {
                    quad_size = custom_size;
                }

                // Apply size and global transform
                let positions = QUAD_VERTEX_POSITIONS.map(|quad_pos| {
                    extracted_sprite
                        .transform
                        .transform_point(
                            ((quad_pos - extracted_sprite.anchor) * quad_size).extend(0.),
                        )
                        .into()
                });

                // These items will be sorted by depth with other phase items
                let sort_key = FloatOrd(extracted_sprite.transform.translation().z);

                // Store the vertex data and add the item to the render phase
                for i in QUAD_INDICES {
                    sprite_meta.vertices.push(TextModeSpriteVertex {
                        position: positions[i],
                        uv: uvs[i].into(),
                        bg: extracted_sprite.bg.as_linear_rgba_f32(),
                        fg: extracted_sprite.fg.as_linear_rgba_f32(),
                    });
                }
                let item_start = index;
                index += QUAD_INDICES.len() as u32;
                let item_end = index;

                transparent_phase.add(Transparent2d {
                    draw_function: draw_sprite_function,
                    pipeline,
                    entity: current_batch_entity,
                    sort_key,
                    batch_range: Some(item_start..item_end),
                });
            }
        }
        sprite_meta
            .vertices
            .write_buffer(&render_device, &render_queue);
    }
}

pub type DrawTextModeSprite = (
    SetItemPipeline,
    SetTextModeSpriteViewBindGroup<0>,
    SetTextModeSpriteTextureBindGroup<1>,
    DrawTextModeSpriteBatch,
);

pub struct SetTextModeSpriteViewBindGroup<const I: usize>;
impl<const I: usize> EntityRenderCommand for SetTextModeSpriteViewBindGroup<I> {
    type Param = (SRes<TextModeSpriteMeta>, SQuery<Read<ViewUniformOffset>>);

    fn render<'w>(
        view: Entity,
        _item: Entity,
        (sprite_meta, view_query): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let view_uniform = view_query.get(view).unwrap();
        pass.set_bind_group(
            I,
            sprite_meta.into_inner().view_bind_group.as_ref().unwrap(),
            &[view_uniform.offset],
        );
        RenderCommandResult::Success
    }
}
pub struct SetTextModeSpriteTextureBindGroup<const I: usize>;
impl<const I: usize> EntityRenderCommand for SetTextModeSpriteTextureBindGroup<I> {
    type Param = (SRes<TextModeImageBindGroups>, SQuery<Read<TextModeSpriteBatch>>);

    fn render<'w>(
        _view: Entity,
        item: Entity,
        (image_bind_groups, query_batch): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let sprite_batch = query_batch.get(item).unwrap();
        let image_bind_groups = image_bind_groups.into_inner();

        pass.set_bind_group(
            I,
            image_bind_groups
                .values
                .get(&Handle::weak(sprite_batch.image_handle_id))
                .unwrap(),
            &[],
        );
        RenderCommandResult::Success
    }
}

pub struct DrawTextModeSpriteBatch;
impl<P: BatchedPhaseItem> RenderCommand<P> for DrawTextModeSpriteBatch {
    type Param = SRes<TextModeSpriteMeta>;

    fn render<'w>(
        _view: Entity,
        item: &P,
        sprite_meta: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let sprite_meta = sprite_meta.into_inner();
        pass.set_vertex_buffer(0, sprite_meta.vertices.buffer().unwrap().slice(..));
        pass.draw(item.batch_range().as_ref().unwrap().clone(), 0..1);
        RenderCommandResult::Success
    }
}