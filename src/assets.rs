//! Embedded icon assets shared by the whole UI.

use crate::prelude::*;

// ───────────────────────────── assets ─────────────────────────────

// Kit's default source only embeds 101 icons; pick exactly the extra Lucide ones we use.
// (A misspelt / missing variant is a compile error, so swap for the nearest Lucide name.)
icon_assets!(
    ExtraIcons,
    [
        Phone,
        Users,
        MessageSquare,
        Mail,
        CalendarDays,
        Image,
        Music,
        ShoppingBag,
        Sun,
        Map,
        Globe,
        Clock,
        Settings,
        Gamepad2,
        Camera,
        StickyNote,
        ArrowLeft,
        ArrowRight,
        ArrowLeftRight,
        Search,
        X,
        ChevronUp,
        Wifi,
        WifiOff,
        Bluetooth,
        BluetoothOff,
        Plane,
        Bell,
        BellOff,
        Moon,
        Battery,
        BatteryCharging,
        Ellipsis,
        Lock,
        Power,
        Delete
    ]
);

pub(crate) struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        ComponentAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = ComponentAssets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
