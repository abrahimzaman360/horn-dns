# How to Wire Lock Screen into Project:

Step 2: Wire it into main.rs:

Copy// next to the other top-level items
mod lock;
use lock::{DEFAULT_PIN, LockState, hash_pin};

// icons: add to your icon_assets! list
//   Lock, Delete
Copy// ── Config: three new fields ──
#[serde(default = "default_pin_hash")]
pin_hash: Option<String>,        // hex FNV hash; null = no PIN
#[serde(default = "default_pin_len")]
pin_len: usize,
#[serde(default)]
lock_wallpaper: Option<String>,  // None = follow Start wallpaper

fn default_pin_hash() -> Option<String> {
    Some(format!("{:016x}", hash_pin(DEFAULT_PIN)))
}
fn default_pin_len() -> usize { 4 }

// Config::default():
pin_hash: default_pin_hash(), pin_len: 4, lock_wallpaper: None,

// Config::sanitized():
self.pin_len = self.pin_len.clamp(4, 6);
self.lock_wallpaper = self.lock_wallpaper.filter(|p| resolve_wallpaper(p).is_some());
Copy// ── struct Metro ── (delete the lock fields from my previous answer, keep only this)
lock: LockState,

// ── Metro::new ── (before `let mut me = Metro { ... }`, then add the field)
let lock = LockState::new(&config, cx);
// ...inside the struct literal:
lock,

// ── Metro::config() ── add to the Config literal:
pin_hash: self.lock.pin_hash.map(|h| format!("{h:016x}")),
pin_len: self.lock.pin_len,
lock_wallpaper: self
    .lock
    .wallpaper
    .as_ref()
    .map(|p| p.to_string_lossy().to_string()),

// ── reset_config() ── add:
self.lock.reset_defaults();
Copy// ── nothing navigates while locked ──
fn go(&mut self, page: Page, cx: &mut Context<Self>) {
    if self.lock.locked || self.page == page { return; }
    // ...unchanged
}

// ── Back goes to the lock screen first ──
fn back(&mut self, cx: &mut Context<Self>) {
    if self.lock_back(cx) { return; }
    // ...unchanged
}

// ── system_panel(): Lock button ──
let lock_btn = panel_button("panel-lock", IconName::Lock, "Lock")
    .on_click(cx.listener(|this, _, _, cx| this.lock_now(cx)));
// ...then in the bottom row: .child(lock_btn).child(theme_btn).child(settings_btn)

// ── settings_page(): inside the column ──
.child(self.lock_settings_section(cx))
Copy// ── render() ──
self.maybe_focus_search(window, cx);
self.lock_prepare(window, cx);                    // <- add
// ...
let launch_layer = self.launch_layer(width);
let lock_layer = self.lock_layer(window, cx);     // <- add

// root chain, right after the launch layer:
.when_some(launch_layer, |d, layer| d.child(layer))
.when_some(lock_layer, |d, layer| d.child(layer)) // <- add
// brightness dim → panel_layer → ... → status_bar stay exactly as they are
