use bevy::prelude::*;
use bevy::sprite::Anchor;

#[derive(Component, Debug, Clone, Reflect)]
pub struct TextModeTextureAtlasSprite {
    pub bg: Color,
    pub fg: Color,
    pub index: usize,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
    pub custom_size: Option<Vec2>,
    pub anchor: Anchor,
}

impl Default for TextModeTextureAtlasSprite {
    fn default() -> Self {
        Self {
            index: 0,
            bg: Color::WHITE,
            fg: Color::BLACK,
            flip_x: false,
            flip_y: false,
            rotation: 0,
            custom_size: None,
            anchor: Anchor::default(),
        }
    }
}

#[derive(Bundle, Clone, Default)]
pub struct TextModeSpriteSheetBundle {
    pub sprite: TextModeTextureAtlasSprite,
    pub texture_atlas: Handle<TextureAtlas>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub computed_visibility: ComputedVisibility,
}