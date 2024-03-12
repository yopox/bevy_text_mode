use bevy::prelude::*;
use bevy::utils::HashSet;
use bevy_sprite::ImageScaleMode;

use crate::plugin::TextModeExtractedSprite;
use crate::TextModeSprite;

/// Component storing texture slices for sprite entities with a [`ImageScaleMode`]
///
/// This component is automatically inserted and updated
#[derive(Debug, Clone, Component)]
pub struct ComputedTextModeTextureSlices(Vec<TextureSlice>);

impl ComputedTextModeTextureSlices {
    /// Computes [`TextModeExtractedSprite`] iterator from the sprite slices
    ///
    /// # Arguments
    ///
    /// * `transform` - the sprite entity global transform
    /// * `original_entity` - the sprite entity
    /// * `sprite` - The sprite component
    /// * `handle` - The sprite texture handle
    #[must_use]
    pub(crate) fn extract_text_mode_sprites<'a>(
        &'a self,
        transform: &'a GlobalTransform,
        original_entity: Entity,
        sprite: &'a TextModeSprite,
        handle: &'a Handle<Image>,
    ) -> impl ExactSizeIterator<Item = TextModeExtractedSprite> + 'a {
        let mut flip = Vec2::ONE;
        let [mut flip_x, mut flip_y] = [false; 2];
        if sprite.flip_x {
            flip.x *= -1.0;
            flip_x = true;
        }
        if sprite.flip_y {
            flip.y *= -1.0;
            flip_y = true;
        }
        self.0.iter().map(move |slice| {
            let offset = (slice.offset * flip).extend(0.0);
            let transform = transform.mul_transform(Transform::from_translation(offset));
            TextModeExtractedSprite {
                transform,
                bg: sprite.bg,
                fg: sprite.fg,
                alpha: sprite.alpha,
                custom_size: Some(slice.draw_size),
                image_handle_id: handle.id(),
                flip_x,
                flip_y,
                rotation: sprite.rotation,
                anchor: sprite.anchor.as_vec(),
                rect: sprite.rect,
                original_entity: Some(original_entity),
            }
        })
    }
}

/// Generates sprite slices for a `sprite` given a `scale_mode`. The slices
/// will be computed according to the `image_handle` dimensions or the sprite rect.
///
/// Returns `None` if the image asset is not loaded
#[must_use]
fn compute_text_mode_sprite_slices(
    sprite: &TextModeSprite,
    scale_mode: &ImageScaleMode,
    image_handle: &Handle<Image>,
    images: &Assets<Image>,
) -> Option<ComputedTextModeTextureSlices> {
    let image_size = images.get(image_handle).map(|i| {
        Vec2::new(
            i.texture_descriptor.size.width as f32,
            i.texture_descriptor.size.height as f32,
        )
    })?;
    let slices = match scale_mode {
        ImageScaleMode::Sliced(slicer) => slicer.compute_slices(
            Rect {
                min: Vec2::ZERO,
                max: image_size,
            },
            sprite.custom_size,
        ),
        ImageScaleMode::Tiled {
            tile_x,
            tile_y,
            stretch_value,
        } => {
            let slice = TextureSlice {
                texture_rect: Rect {
                    min: Vec2::ZERO,
                    max: image_size,
                },
                draw_size: sprite.custom_size.unwrap_or(image_size),
                offset: Vec2::ZERO,
            };
            slice.tiled(*stretch_value, (*tile_x, *tile_y))
        }
    };
    Some(ComputedTextModeTextureSlices(slices))
}

/// System reacting to added or modified [`Image`] handles, and recompute sprite slices
/// on matching sprite entities with a [`ImageScaleMode`] component
pub(crate) fn compute_text_mode_slices_on_asset_event(
    mut commands: Commands,
    mut events: EventReader<AssetEvent<Image>>,
    images: Res<Assets<Image>>,
    sprites: Query<(Entity, &ImageScaleMode, &TextModeSprite, &Handle<Image>)>,
) {
    // We store the asset ids of added/modified image assets
    let added_handles: HashSet<_> = events
        .read()
        .filter_map(|e| match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    if added_handles.is_empty() {
        return;
    }
    // We recompute the sprite slices for sprite entities with a matching asset handle id
    for (entity, scale_mode, sprite, image_handle) in &sprites {
        if !added_handles.contains(&image_handle.id()) {
            continue;
        }
        if let Some(slices) = compute_text_mode_sprite_slices(sprite, scale_mode, image_handle, &images) {
            commands.entity(entity).insert(slices);
        }
    }
}

/// System reacting to changes on relevant sprite bundle components to compute the sprite slices
/// on matching sprite entities with a [`ImageScaleMode`] component
pub(crate) fn compute_text_mode_slices_on_sprite_change(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    changed_sprites: Query<
        (Entity, &ImageScaleMode, &TextModeSprite, &Handle<Image>),
        Or<(
            Changed<ImageScaleMode>,
            Changed<Handle<Image>>,
            Changed<TextModeSprite>,
        )>,
    >,
) {
    for (entity, scale_mode, sprite, image_handle) in &changed_sprites {
        if let Some(slices) = compute_text_mode_sprite_slices(sprite, scale_mode, image_handle, &images) {
            commands.entity(entity).insert(slices);
        }
    }
}
