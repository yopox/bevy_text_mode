use bevy::prelude::*;
use bevy::sprite::Anchor;

use bevy_text_mode::{TextModePlugin, TextModeSprite, TextModeSpriteBundle};

const WIDTH: f32 = 8. * 8. * 8.;
const HEIGHT: f32 = 7. * 8. * 8.;

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
        .add_plugins(TextModePlugin)
        .add_systems(Startup, init)
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
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let tileset: Handle<Image> = server.load("texmod.png");
    let layout = TextureAtlasLayout::from_grid(Vec2::new(8., 8.), 7, 1, None, None);
    let handle = texture_atlas_layouts.add(layout);

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
        commands.spawn(TextModeSpriteBundle {
            sprite: TextModeSprite {
                bg,
                fg,
                anchor: Anchor::TopLeft,
                ..default()
            },
            atlas: TextureAtlas {
                layout: handle.clone(),
                index: i,
            },
            texture: tileset.clone(),
            transform: Transform::from_xyz(8. * x as f32, -8. * y as f32, 0.),
            ..default()
        });
    }

    for (x, y, i, flip_x, flip_y, rotation) in [
        (1, 4, 6, false, false, 0),
        (2, 4, 6, false, false, 1),
        (3, 4, 6, false, false, 2),
        (4, 4, 6, false, false, 3),

        (3, 5, 6, false, false, 0),
        (4, 5, 6, true, false, 0),
        (5, 5, 6, false, true, 0),
        (6, 5, 6, true, true, 0),
    ] {
        commands.spawn(TextModeSpriteBundle {
            sprite: TextModeSprite {
                bg: Light::WHITE.into(),
                fg: Dark::BLACK.into(),
                flip_x,
                flip_y,
                rotation,
                anchor: Anchor::TopLeft,
                ..default()
            },
            atlas: TextureAtlas {
                layout: handle.clone(),
                index: i,
            },
            texture: tileset.clone(),
            transform: Transform::from_xyz(8. * x as f32, -8. * y as f32, 0.),
            ..default()
        });
    }
}