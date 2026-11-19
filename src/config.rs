//! Persisted settings: defaults, sanitising and disk I/O.

use crate::prelude::*;

// ───────────────────────────── persisted config ─────────────────────────────

/// Bumped when the on-disk shape needs migrating.
pub(crate) const CONFIG_VERSION: u32 = 2;

/// Everything that survives a restart. Stored as pretty JSON so it is easy to
/// hand-edit; unknown or broken values fall back to the defaults below.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    #[serde(default = "config_version")]
    pub(crate) version: u32,
    #[serde(default = "default_dark")]
    pub(crate) dark: bool,
    #[serde(default = "default_accent")]
    pub(crate) accent: u32,
    #[serde(default)]
    pub(crate) wallpaper: usize,
    /// Absolute or `wallpapers/…` path to a wallpaper image; wins over the preset.
    #[serde(default)]
    pub(crate) wallpaper_file: Option<String>,
    #[serde(default = "default_gap")]
    pub(crate) gap: f32,
    #[serde(default = "default_alpha")]
    pub(crate) tile_alpha: f32,
    #[serde(default)]
    pub(crate) liquid_glass: bool,
    #[serde(default = "default_auto_adjust")]
    pub(crate) auto_adjust_tiles: bool,
    #[serde(default)]
    pub(crate) tile_borders: bool,
    #[serde(default)]
    pub(crate) force_accent_live_tiles: bool,
    #[serde(default)]
    pub(crate) app_sort: AppSort,
    #[serde(default = "default_pin_hash")]
    pub(crate) pin_hash: Option<String>,
    #[serde(default = "default_pin_len")]
    pub(crate) pin_len: usize,
    #[serde(default)]
    pub(crate) lock_wallpaper: Option<String>,
    #[serde(default = "default_brightness")]
    pub(crate) brightness: f32,
    /// Start-screen tiles in display order; the first-fit packer places them.
    #[serde(default = "default_tiles")]
    pub(crate) tiles: Vec<TileConfig>,
}

/// App names (not indices) are stored, so the file stays readable and survives
/// the app list being re-ordered in a future build.
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct TileConfig {
    pub(crate) app: String,
    #[serde(default = "default_tile_size")]
    pub(crate) size: TileSize,
    #[serde(default = "default_live_enabled")]
    pub(crate) live_enabled: bool,
    #[serde(default)]
    pub(crate) manual_x: Option<f32>,
    #[serde(default)]
    pub(crate) manual_y: Option<f32>,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AppSort {
    #[default]
    Category,
    Alphabetical,
}

pub(crate) fn config_version() -> u32 {
    CONFIG_VERSION
}

pub(crate) fn default_dark() -> bool {
    true
}

pub(crate) fn default_accent() -> u32 {
    0xA4C400
}

pub(crate) fn default_gap() -> f32 {
    DEFAULT_GAP
}

pub(crate) fn default_alpha() -> f32 {
    1.0
}

pub(crate) fn default_auto_adjust() -> bool {
    true
}

pub(crate) fn default_live_enabled() -> bool {
    true
}

pub(crate) fn default_brightness() -> f32 {
    1.0
}

pub(crate) fn default_pin_hash() -> Option<String> {
    Some(format!("{:016x}", hash_pin(DEFAULT_PIN)))
}

pub(crate) fn default_pin_len() -> usize {
    4
}

pub(crate) fn default_tile_size() -> TileSize {
    TileSize::Medium
}

pub(crate) fn default_tiles() -> Vec<TileConfig> {
    use TileSize::*;
    [
        ("Phone", Medium),
        ("People", Medium),
        ("Messaging", Wide),
        ("Mail", Medium),
        ("Calendar", Medium),
        ("Weather", Wide),
        ("Clock", Medium),
        ("Music", Medium),
        ("Photos", Wide),
        ("Store", Medium),
        ("Maps", Small),
        ("Internet", Small),
        ("Camera", Small),
        ("Notes", Small),
        ("Settings", Medium),
        ("Games", Wide),
    ]
    .into_iter()
    .map(|(app, size)| TileConfig {
        app: app.to_string(),
        size,
        live_enabled: true,
        manual_x: None,
        manual_y: None,
    })
    .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            dark: default_dark(),
            accent: default_accent(),
            wallpaper: 0,
            wallpaper_file: None,
            gap: DEFAULT_GAP,
            tile_alpha: 1.0,
            liquid_glass: false,
            auto_adjust_tiles: true,
            tile_borders: false,
            force_accent_live_tiles: false,
            app_sort: AppSort::Category,
            pin_hash: default_pin_hash(),
            pin_len: 4,
            lock_wallpaper: None,
            brightness: 1.0,
            tiles: default_tiles(),
        }
    }
}

impl Config {
    /// Clamp hand-edited values and drop tiles that name an unknown app.
    pub(crate) fn sanitized(mut self) -> Self {
        self.wallpaper = self.wallpaper.min(WALLPAPER_NAMES.len() - 1);
        self.gap = self.gap.clamp(GAP_MIN, GAP_MAX);
        self.tile_alpha = self.tile_alpha.clamp(TILE_ALPHA_MIN, 1.0);
        for tile in &mut self.tiles {
            tile.manual_x = tile.manual_x.map(|x| x.clamp(0.0, 1.0));
            tile.manual_y = tile.manual_y.map(|y| y.clamp(0.0, MAX_MANUAL_ROWS as f32));
        }
        self.brightness = self.brightness.clamp(0.05, 1.0);
        self.pin_len = self.pin_len.clamp(4, 6);
        self.lock_wallpaper = self
            .lock_wallpaper
            .filter(|path| resolve_wallpaper(path).is_some());
        self.wallpaper_file = self
            .wallpaper_file
            .filter(|path| resolve_wallpaper(path).is_some());
        self.tiles.retain(|tile| app_index(&tile.app).is_some());
        if self.tiles.is_empty() {
            self.tiles = default_tiles();
        }
        self
    }
}

/// `$HORN_CONFIG` wins, then the usual per-OS user config directory:
/// `%APPDATA%\horn\config.json`, `~/Library/Application Support/horn/config.json`
/// or `$XDG_CONFIG_HOME|~/.config/horn/config.json`.
pub(crate) fn config_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HORN_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    Some(base.join("horn").join("config.json"))
}

pub(crate) fn load_config() -> Option<Config> {
    let path = config_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<Config>(&text) {
        Ok(config) => Some(config.sanitized()),
        Err(err) => {
            eprintln!("horn: ignoring unreadable {}: {err}", path.display());
            None
        }
    }
}

/// Write to a temp file and rename over the old one, so an interrupted save
/// can never leave a truncated config behind.
pub(crate) fn save_config(config: &Config) {
    let Some(path) = config_path() else { return };
    let Some(dir) = path.parent() else { return };
    if let Err(err) = std::fs::create_dir_all(dir) {
        eprintln!("horn: could not create {}: {err}", dir.display());
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(config) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(err) = std::fs::write(&tmp, json) {
        eprintln!("horn: could not write {}: {err}", tmp.display());
        return;
    }
    if let Err(err) = std::fs::rename(&tmp, &path) {
        eprintln!("horn: could not replace {}: {err}", path.display());
    }
}
