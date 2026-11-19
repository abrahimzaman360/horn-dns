//! Unit tests for configuration, wallpapers and the static app catalogue.

use crate::motion::{blend_hex, prefers_white, prefers_white_tile};

use crate::config::{AppSort, CONFIG_VERSION, Config, default_tiles, load_config, save_config};
use crate::data::{GAP_MAX, TILE_ALPHA_MIN, WALLPAPER_NAMES, app_index};
use crate::model::TileSize;
use crate::wallpapers::{is_image_file, wallpaper_label};

#[test]
fn image_file_detection_and_labels() {
    use std::path::Path;
    assert!(is_image_file(Path::new("a/b/Sunset.PNG")));
    assert!(is_image_file(Path::new("x.jpeg")));
    assert!(is_image_file(Path::new("x.webp")));
    assert!(!is_image_file(Path::new("notes.txt")));
    assert!(!is_image_file(Path::new("no-extension")));
    assert_eq!(
        wallpaper_label(Path::new("wallpapers/quiet-woods.png")),
        "quiet-woods"
    );
}

#[test]
fn default_tiles_reference_real_apps() {
    let tiles = default_tiles();
    assert!(!tiles.is_empty());
    for tile in &tiles {
        assert!(
            app_index(&tile.app).is_some(),
            "{} is not in the app list",
            tile.app
        );
    }
}

#[test]
fn config_round_trips_through_json() {
    let mut config = Config {
        dark: false,
        accent: 0xE51400,
        wallpaper: 3,
        gap: 14.5,
        tile_alpha: 0.75,
        brightness: 0.6,
        liquid_glass: true,
        auto_adjust_tiles: false,
        tile_borders: true,
        force_accent_live_tiles: true,
        app_sort: AppSort::Alphabetical,
        ..Config::default()
    };
    config.tiles[0].size = TileSize::Large;
    config.tiles[0].live_enabled = false;
    config.tiles[0].manual_x = Some(0.25);
    config.tiles[0].manual_y = Some(3.0);

    let json = serde_json::to_string(&config).expect("serialise");
    let parsed: Config = serde_json::from_str(&json).expect("parse");

    assert_eq!(parsed.version, CONFIG_VERSION);
    assert!(!parsed.dark);
    assert_eq!(parsed.accent, 0xE51400);
    assert_eq!(parsed.wallpaper, 3);
    assert_eq!(parsed.gap, 14.5);
    assert_eq!(parsed.tile_alpha, 0.75);
    assert_eq!(parsed.brightness, 0.6);
    assert!(parsed.liquid_glass);
    assert!(!parsed.auto_adjust_tiles);
    assert!(parsed.tile_borders);
    assert!(parsed.force_accent_live_tiles);
    assert_eq!(parsed.app_sort, AppSort::Alphabetical);
    assert_eq!(parsed.tiles.len(), config.tiles.len());
    assert_eq!(parsed.tiles[0].app, config.tiles[0].app);
    assert!(matches!(parsed.tiles[0].size, TileSize::Large));
    assert!(!parsed.tiles[0].live_enabled);
    assert_eq!(parsed.tiles[0].manual_x, Some(0.25));
    assert_eq!(parsed.tiles[0].manual_y, Some(3.0));
}

#[test]
fn sanitize_drops_unknown_apps_and_clamps_values() {
    let json = r#"{
            "accent": 1193046,
            "wallpaper": 99,
            "gap": 500.0,
            "tile_alpha": -1.0,
            "brightness": 5.0,
            "tiles": [
                { "app": "Clock", "size": "large" },
                { "app": "No Such App", "size": "small" }
            ]
        }"#;
    let config: Config = serde_json::from_str(json).expect("parse");
    let config = config.sanitized();

    // Missing fields fall back to defaults.
    assert!(config.dark);
    assert_eq!(config.wallpaper, WALLPAPER_NAMES.len() - 1);
    assert_eq!(config.gap, GAP_MAX);
    assert_eq!(config.tile_alpha, TILE_ALPHA_MIN);
    assert_eq!(config.brightness, 1.0);
    assert!(!config.liquid_glass);
    assert!(config.auto_adjust_tiles);
    // The bogus tile is dropped, the real one is kept.
    assert_eq!(config.tiles.len(), 1);
    assert_eq!(config.tiles[0].app, "Clock");
    assert!(config.tiles[0].live_enabled);
    assert_eq!(config.tiles[0].manual_x, None);
    assert_eq!(config.tiles[0].manual_y, None);
}

#[test]
fn save_then_load_round_trip() {
    let path = std::env::temp_dir().join(format!(
        "horn-config-test-{}-{}.json",
        std::process::id(),
        CONFIG_VERSION
    ));
    // SAFETY: this test owns the variable for its duration; nothing else reads it.
    unsafe {
        std::env::set_var("HORN_CONFIG", &path);
    }

    let mut config = Config {
        accent: 0x008A00,
        gap: 4.0,
        wallpaper: 2,
        ..Config::default()
    };
    config.tiles.retain(|tile| tile.app != "Games");

    save_config(&config);
    let loaded = load_config().expect("config should load back");

    assert_eq!(loaded.accent, 0x008A00);
    assert_eq!(loaded.gap, 4.0);
    assert_eq!(loaded.wallpaper, 2);
    assert!(!loaded.tiles.iter().any(|tile| tile.app == "Games"));

    assert!(
        path.exists(),
        "config file should exist at {}",
        path.display()
    );
    let _ = std::fs::remove_file(&path);
    unsafe {
        std::env::remove_var("HORN_CONFIG");
    }
}

#[test]
fn ink_uses_wcag_contrast_on_tiles() {
    // Dark blue/purple swatches need white type, bright lime/gold need black.
    assert!(prefers_white(0x0050EF), "dark blue should use white ink");
    assert!(prefers_white(0xAA00FF), "purple should use white ink");
    assert!(!prefers_white(0xA4C400), "lime should use black ink");
    assert!(!prefers_white(0xE3C800), "gold should use black ink");

    // A translucent dark-blue tile over a dark wallpaper stays dark, so it
    // must still choose white even though the raw app swatch was lighter.
    let dark_wallpaper = 0x0B1B2B;
    let blue = 0x0050EF;
    assert!(prefers_white(blend_hex(dark_wallpaper, blue, 0.30)));

    // The inverse case: a translucent bright accent over a light wallpaper
    // must choose black, not white.
    let light_wallpaper = 0xE7EEF5;
    assert!(!prefers_white(blend_hex(light_wallpaper, 0xA4C400, 0.30)));
}

#[test]
fn metro_tiles_use_white_on_coloured_plates() {
    // These are the live-tile swatches visible in the Start screen. Metro uses
    // white type on saturated colours, even when black scores marginally better
    // on a raw WCAG calculation.
    for hex in [
        0x0050EF, // Calendar / Internet blue
        0x1BA1E2, // Weather blue
        0x00ABA9, // Clock teal
        0xFA6800, // Music orange
        0x60A917, // Store / Games green
        0x008A00, // Photos green
        0x647687, // Settings grey-blue
        0x6D8764, // muted green
        0xAA00FF, // Maps purple
    ] {
        assert!(
            prefers_white_tile(hex),
            "{hex:06X} should use white tile ink"
        );
    }

    // Light plates keep black ink so the labels do not wash out.
    for hex in [
        0xA4C400, // Lime accent (Phone/People/Messaging/Mail)
        0xF0A30A, // Notes orange-gold
        0xE3C800, // Gold accent
        0xF472D0, // Light pink accent
    ] {
        assert!(
            !prefers_white_tile(hex),
            "{hex:06X} should use black tile ink"
        );
    }
}
