use std::path::PathBuf;

pub const DEFAULT_PIN: &str = "1234";

pub fn hash_pin(pin: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in pin.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub struct LockState {
    pub locked: bool,
    pub pin_hash: Option<u64>,
    pub pin_len: usize,
    pub wallpaper: Option<PathBuf>,
    pub entry: String,
    pub error: bool,
    pub pin_prompt: bool,
    pub swipe_start: Option<f32>,
    pub swipe_offset: f32,
    pub unlocking: bool,
    pub unlock_n: u64,
    pub unlock_scheduled: bool,
    pub overlay_returning: bool,
    pub overlay_return_n: u64,
    pub overlay_return_scheduled: bool,
    pub overlay_revealing: bool,
    pub overlay_reveal_scheduled: bool,
    pub spring_back: bool,
    pub spring_back_n: u64,
    pub spring_back_scheduled: bool,
}

impl LockState {
    pub fn new(
        pin_hash: Option<String>,
        pin_len: usize,
        lock_wallpaper: Option<&str>,
        resolve_wallpaper: impl Fn(&str) -> Option<PathBuf>,
    ) -> Self {
        let pin_hash = pin_hash.and_then(|value| u64::from_str_radix(&value, 16).ok());
        let wallpaper = lock_wallpaper.and_then(resolve_wallpaper);
        Self {
            locked: pin_hash.is_some(),
            pin_hash,
            pin_len: pin_len.clamp(4, 6),
            wallpaper,
            entry: String::new(),
            error: false,
            pin_prompt: false,
            swipe_start: None,
            swipe_offset: 0.0,
            unlocking: false,
            unlock_n: 0,
            unlock_scheduled: false,
            overlay_returning: false,
            overlay_return_n: 0,
            overlay_return_scheduled: false,
            overlay_revealing: false,
            overlay_reveal_scheduled: false,
            spring_back: false,
            spring_back_n: 0,
            spring_back_scheduled: false,
        }
    }

    pub fn lock_now(&mut self) {
        if self.pin_hash.is_some() {
            self.locked = true;
            self.entry.clear();
            self.error = false;
            self.pin_prompt = false;
            self.swipe_start = None;
            self.swipe_offset = 0.0;
            self.unlocking = false;
            self.unlock_scheduled = false;
            self.overlay_revealing = false;
            self.overlay_reveal_scheduled = false;
            self.overlay_returning = false;
            self.overlay_return_scheduled = false;
            self.spring_back = false;
            self.spring_back_scheduled = false;
            self.overlay_returning = false;
            self.overlay_return_scheduled = false;
            self.overlay_returning = false;
            self.overlay_return_scheduled = false;
            self.overlay_revealing = false;
            self.overlay_reveal_scheduled = false;
            self.spring_back = false;
            self.spring_back_scheduled = false;
        }
    }

    pub fn reset_defaults(&mut self) {
        self.pin_hash = Some(hash_pin(DEFAULT_PIN));
        self.pin_len = 4;
        self.wallpaper = None;
        self.entry.clear();
        self.error = false;
        self.locked = true;
        self.pin_prompt = false;
        self.swipe_start = None;
        self.swipe_offset = 0.0;
        self.unlocking = false;
        self.unlock_scheduled = false;
        self.overlay_revealing = false;
        self.overlay_reveal_scheduled = false;
        self.overlay_returning = false;
        self.overlay_return_scheduled = false;
        self.spring_back = false;
        self.spring_back_scheduled = false;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.pin_hash = enabled.then(|| hash_pin(DEFAULT_PIN));
        self.locked = enabled;
        self.entry.clear();
        self.error = false;
        self.pin_prompt = false;
        self.swipe_start = None;
        self.swipe_offset = 0.0;
        self.unlocking = false;
        self.unlock_scheduled = false;
    }

    pub fn add_digit(&mut self, digit: char) {
        if !digit.is_ascii_digit() || self.entry.len() >= self.pin_len {
            return;
        }
        self.entry.push(digit);
        self.error = false;
    }

    pub fn backspace(&mut self) {
        self.entry.pop();
        self.error = false;
    }

    pub fn back_to_overlay(&mut self) -> bool {
        if !self.pin_prompt {
            return false;
        }
        self.pin_prompt = false;
        self.entry.clear();
        self.error = false;
        self.overlay_returning = true;
        self.overlay_return_n = self.overlay_return_n.wrapping_add(1);
        self.overlay_return_scheduled = false;
        true
    }

    pub fn finish_overlay_return(&mut self) {
        self.overlay_returning = false;
        self.overlay_return_scheduled = false;
        self.swipe_offset = 0.0;
    }

    pub fn finish_spring_back(&mut self) {
        self.spring_back = false;
        self.spring_back_scheduled = false;
        self.swipe_offset = 0.0;
        self.swipe_start = None;
    }

    pub fn begin_swipe(&mut self, y: f32) {
        self.swipe_start = Some(y);
        self.swipe_offset = 0.0;
        self.spring_back = false;
        self.overlay_revealing = false;
    }

    pub fn move_swipe(&mut self, y: f32) {
        if let Some(start) = self.swipe_start {
            let upward = start - y;
            self.swipe_offset = upward.clamp(0.0, 720.0);
        }
    }

    pub fn end_swipe(&mut self, viewport_height: f32) -> bool {
        let threshold = viewport_height * 0.50;
        let intentional = self.swipe_offset >= threshold;
        self.swipe_start = None;
        if intentional {
            self.spring_back = false;
            if self.pin_hash.is_some() {
                self.pin_prompt = true;
                self.overlay_revealing = true;
                self.overlay_reveal_scheduled = false;
            } else {
                self.unlocking = true;
                self.unlock_n = self.unlock_n.wrapping_add(1);
                self.unlock_scheduled = false;
            }
        } else {
            self.spring_back = true;
            self.spring_back_n = self.spring_back_n.wrapping_add(1);
            self.spring_back_scheduled = false;
        }
        intentional
    }

    pub fn complete_unlock(&mut self) {
        self.locked = false;
        self.pin_prompt = false;
        self.unlocking = false;
        self.unlock_scheduled = false;
        self.swipe_offset = 0.0;
        self.entry.clear();
        self.error = false;
        self.spring_back = false;
        self.spring_back_scheduled = false;
    }

    pub fn verify(&mut self) -> bool {
        let Some(expected) = self.pin_hash else {
            self.locked = false;
            return true;
        };
        if self.entry.len() == self.pin_len && hash_pin(&self.entry) == expected {
            self.locked = false;
            self.entry.clear();
            self.error = false;
            self.pin_prompt = false;
            self.unlocking = false;
            self.swipe_offset = 0.0;
            true
        } else {
            self.error = true;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swipe_requires_intentional_upward_distance() {
        let pin_hash = Some(format!("{:016x}", hash_pin(DEFAULT_PIN)));
        let mut lock = LockState::new(pin_hash, 4, None, |_| None);

        lock.begin_swipe(500.0);
        lock.move_swipe(460.0);
        assert!(!lock.end_swipe(800.0));
        assert!(!lock.pin_prompt);

        lock.begin_swipe(500.0);
        lock.move_swipe(50.0);
        assert!(lock.end_swipe(800.0));
        assert!(lock.pin_prompt);

        let mut short_lock = LockState::new(
            Some(format!("{:016x}", hash_pin(DEFAULT_PIN))),
            4,
            None,
            |_| None,
        );
        short_lock.begin_swipe(500.0);
        short_lock.move_swipe(420.0);
        assert!(!short_lock.end_swipe(800.0));
        assert!(short_lock.spring_back);
    }

    #[test]
    fn pin_verification_accepts_default_pin() {
        let pin_hash = Some(format!("{:016x}", hash_pin(DEFAULT_PIN)));
        let mut lock = LockState::new(pin_hash, 4, None, |_| None);
        for digit in DEFAULT_PIN.chars() {
            lock.add_digit(digit);
        }
        assert!(lock.verify());
        assert!(!lock.locked);
    }
}
