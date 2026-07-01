# bevy_text_mode

[![bevy](https://img.shields.io/badge/bevy-v0.19.0-blue.svg)](https://github.com/bevyengine/bevy)

<p align="center">
    <img src="https://raw.githubusercontent.com/yopox/bevy_text_mode/main/assets/promo.png" />
</p>

> `bevy_text_mode` adds a `TextModeSprite` component with configurable background and foreground colors.
It makes it easy to use text mode tilesets such as [MRMOTEXT](https://mrmotarius.itch.io/mrmotext).

```rust
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
```

## Usage

Spawn a `TextModeSprite` (or a `TextModeSpriteBundle`) with the desired background and foreground
colors. As with `bevy`'s own `Sprite`, the image and texture atlas are set directly on the
component.

## Compatible Bevy versions

| `bevy_text_mode` | `bevy` |
|:----------------:|:------:|
|      0.5.0       |  0.19  |
|      0.4.0       |  0.14  |
|      0.3.0       |  0.13  |
|      0.2.0       |  0.11  |
|      0.1.1       |  0.10  |
