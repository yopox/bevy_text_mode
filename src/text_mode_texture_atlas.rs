use bevy::camera::visibility::{self, VisibilityClass};
use bevy::image::TextureAtlas;
use bevy::prelude::*;
use bevy::sprite::{Anchor, SpriteImageMode};

#[derive(Component, Debug, Clone, Reflect)]
#[require(Transform, Visibility, VisibilityClass)]
#[component(on_add = visibility::add_visibility_class::<TextModeSprite>)]
pub struct TextModeSprite {
    pub bg: LinearRgba,
    pub fg: LinearRgba,
    pub alpha: f32,
    pub image: Handle<Image>,
    pub texture_atlas: Option<TextureAtlas>,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
    pub custom_size: Option<Vec2>,
    pub rect: Option<Rect>,
    pub image_mode: SpriteImageMode,
    pub anchor: Anchor,
}

impl Default for TextModeSprite {
    fn default() -> Self {
        Self {
            bg: Color::WHITE.to_linear(),
            fg: Color::BLACK.to_linear(),
            alpha: 1.0,
            image: Handle::default(),
            texture_atlas: None,
            flip_x: false,
            flip_y: false,
            rotation: 0,
            custom_size: None,
            rect: None,
            image_mode: SpriteImageMode::default(),
            anchor: Anchor::default(),
        }
    }
}

#[derive(Bundle, Clone, Default)]
pub struct TextModeSpriteBundle {
    pub sprite: TextModeSprite,
    pub transform: Transform,
    pub visibility: Visibility,
}
