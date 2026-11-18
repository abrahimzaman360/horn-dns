# Wallpapers

Drop image files in this folder (PNG, JPEG, WebP, BMP, GIF, TIFF) and they show up
under **Settings → Wallpapers**. Pick one and it is restored on the next launch.

- `HORN_WALLPAPERS=/some/other/dir` points the app at a different folder.
- Images are cover-cropped behind a light accent wash and a scrim so tile text
  stays readable in both dark and light themes.
- The three images here are generated samples; delete them freely.

The selected wallpaper is stored in `config.json` (see `HORN_CONFIG`) as
`wallpaper_file`; selecting a gradient preset clears that field.