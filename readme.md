# bevy_text_mode

<p align="center">
    <img src="https://raw.githubusercontent.com/yopox/bevy_text_mode/main/assets/promo.png" />
</p>

This plugin adds a `TextModeTextureAtlasSprite` component with configurable background and foreground colors.
It makes it easy to use text mode tilesets such as [MRMOTEXT](https://mrmotarius.itch.io/mrmotext).

```rust
pub struct TextModeTextureAtlasSprite {
    pub bg: Color,
    pub fg: Color,
    pub alpha: f32,
    pub index: usize,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
    pub custom_size: Option<Vec2>,
    pub anchor: Anchor,
}
```

## Usage

Spawn a `TextModeSpriteSheetBundle` with desired background and foreground colors.

## Compatible Bevy versions

The main branch is compatible with the latest Bevy release.

Compatibility of `bevy_kira_audio` versions:

| `bevy_text_mode` | `bevy` |
|:----------------:|:------:|
|       0.1        |  0.10  |
