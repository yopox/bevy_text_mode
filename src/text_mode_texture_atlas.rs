use bevy::prelude::*;
use bevy::sprite::Anchor;

#[derive(Component, Debug, Clone, Reflect)]
pub struct TextModeSprite {
    pub bg: LinearRgba,
    pub fg: LinearRgba,
    pub alpha: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
    pub custom_size: Option<Vec2>,
    pub rect: Option<Rect>,
    pub anchor: Anchor,
}

impl Default for TextModeSprite {
    fn default() -> Self {
        Self {
            bg: Color::WHITE.to_linear(),
            fg: Color::BLACK.to_linear(),
            alpha: 1.0,
            flip_x: false,
            flip_y: false,
            rotation: 0,
            custom_size: None,
            rect: None,
            anchor: Anchor::default(),
        }
    }
}

#[derive(Bundle, Clone, Default)]
pub struct TextModeSpriteBundle {
    pub sprite: TextModeSprite,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub texture: Handle<Image>,
    pub atlas: TextureAtlas,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}