use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy_text_mode::{TextModePlugin, TextModeSpriteSheetBundle, TextModeTextureAtlasSprite};

const WIDTH: f32 = 8. * 8. * 8.;
const HEIGHT: f32 = 4. * 8. * 8.;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: (WIDTH, HEIGHT).into(),
                    title: "bevy_text_mode".into(),
                    ..default()
                }),
                ..default()
            })
        )
        .insert_resource(ClearColor(Color::WHITE))
        .add_plugin(TextModePlugin)
        .add_startup_system(init)
        .run();
}

enum Dark {
    BLACK,
    BLUE,
    GREEN,
    ORANGE,
    PINK,
}

impl Into<Color> for Dark {
    fn into(self) -> Color {
        match self {
            Dark::BLACK => Color::BLACK,
            Dark::BLUE => Color::hex("305182").unwrap(),
            Dark::GREEN => Color::hex("386900").unwrap(),
            Dark::ORANGE => Color::hex("a23000").unwrap(),
            Dark::PINK => Color::hex("9a2079").unwrap(),
        }
    }
}

enum Light {
    WHITE,
    BLUE,
    GREEN,
    ORANGE,
    PINK,
}

impl Into<Color> for Light {
    fn into(self) -> Color {
        match self {
            Light::WHITE => Color::WHITE,
            Light::BLUE => Color::hex("a2fff3").unwrap(),
            Light::GREEN => Color::hex("cbf382").unwrap(),
            Light::ORANGE => Color::hex("ffcbba").unwrap(),
            Light::PINK => Color::hex("e3b2ff").unwrap(),
        }
    }
}

fn init(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
) {
    let tileset: Handle<Image> = server.load("texmod.png");
    let texture_atlas = TextureAtlas::from_grid(
        tileset,
        Vec2::new(8.0, 8.0), 6, 1,
        None, None
    );
    let handle = texture_atlases.add(texture_atlas);

    commands.spawn(Camera2dBundle {
        transform: Transform {
            translation: Vec3::new(WIDTH / 8. / 2., -HEIGHT / 8. / 2., 0.),
            scale: Vec3::new(1. / 8., 1. / 8., 1.),
            ..default()
        },
        ..default()
    });

    for (x, y, i, bg, fg) in [
        (1, 1, 0, Light::WHITE.into(), Dark::BLUE.into()),
        (2, 1, 1, Light::GREEN.into(), Dark::GREEN.into()),
        (3, 1, 2, Light::WHITE.into(), Dark::ORANGE.into()),
        (4, 1, 0, Light::PINK.into(), Dark::PINK.into()),

        (3, 2, 3, Light::BLUE.into(), Dark::BLUE.into()),
        (4, 2, 4, Light::WHITE.into(), Dark::GREEN.into()),
        (5, 2, 5, Light::ORANGE.into(), Dark::ORANGE.into()),
        (6, 2, 1, Light::WHITE.into(), Dark::PINK.into()),
    ] {
        commands.spawn(TextModeSpriteSheetBundle {
            sprite: TextModeTextureAtlasSprite {
                index: i,
                bg,
                fg,
                anchor: Anchor::TopLeft,
                ..default()
            },
            texture_atlas: handle.clone(),
            transform: Transform::from_xyz(8. * x as f32, -8. * y as f32, 0.),
            ..default()
        });
    }
}