# Ondrane identity

An original orbit and play symbol: midnight rounded tile, a mint open orbit, a periwinkle signal arc and an ivory play glyph. No cone, no VideoLAN branding. The orbit suggests a quiet space around your media; the open side suggests motion and sound.

Palette: Midnight #101827, Mint #78E3CB, Periwinkle #A6B5FF, Ivory #F3F7FF. Primary app accent: Mint. Rounded geometry, strong silhouette at 24px, no embedded photos, no generated imagery. Icon source: VectorCraft 0.21.1. Motion source: EffectCraft 0.21.1, CPU rendering; a restrained 3-second pulse with static support for reduced motion.

Deliverables: editable native files, static icon and outlined wordmark SVG, animated SVG and vector Lottie, PNG/ICO/ICNS platform exports, and a GIF preview for GitHub (which does not play CSS inside SVG images).

## Editable sources and exports

- `ondrane.vectorcraft`: icon master, four named shapes, all vector.
- `wordmark.vectorcraft`: live Inter wordmark and scaled icon. SVG exports outline the text so they render without installed fonts. `wordmark-light.svg` is for light backgrounds; `wordmark.svg` is for dark backgrounds.
- `ondrane-motion.ecproj`: contains:
  1. **Ondrane — Orbit Pulse** (`512×512`, 30 fps, 3 s): play scales 100→110→100%, mint orbit rotates 0→−7→0°, periwinkle signal fades 100→50→100%. EffectCraft quantises the return key to frame 22 (0.7333 s).
  2. **Ondrane media demo** (`1280×720`, 30 fps, 30 s): widescreen wordmark demo.
  3. **Ondrane — Showcase (EN)** (`1280×720`, 30 fps, 12 s): 4-scene English product presentation combining the Orbit Pulse emblem, Player/Settings screenshots, VectorCraft plates (`showcase-bg.png`, `platforms-card.png`), and SoundCraft soundtrack.
- `ondrane-showcase.scproj` / `ondrane-showcase-audio.wav`: original 3-track stereo ambient soundtrack composed in **SoundCraft** (48 kHz, Warm Pad + Mint Chimes + Sub Pulse, normalized to −0.1 dBFS / −10.8 LUFS).
- `showcase-bg.vectorcraft` / `showcase-bg.png` / `showcase-bg.svg`: `1280×720` Midnight/Mint/Periwinkle orbital showcase backdrop created in **VectorCraft**.
- `platforms-card.vectorcraft` / `platforms-card.png`: `1040×680` (@2x) Windows / Linux / macOS release card created in **VectorCraft**.
- `docs/ondrane-showcase.mp4` & `docs/ondrane-showcase.gif`: H.264 720p30 master video with AAC audio and `960×540` 15 fps palette-optimized GIF preview embedded in `README.md`.
- `ondrane-motion.json`: EffectCraft Lottie export, no embedded raster assets, zero export warnings.
- `ondrane-animated.svg`: the VectorCraft paths with a CSS adaptation of the native EffectCraft keyframes, plus `prefers-reduced-motion`. This is not claimed as a direct animated-SVG export from EffectCraft.
- `ondrane-motion.gif`: 90-frame CPU-rendered EffectCraft preview for GitHub, which does not play CSS animation inside SVG images. GIF cannot itself honour reduced-motion preferences; the SVG and static exports are provided separately.
- `ondrane-symbol.svg`: monochrome UI symbol, with the tile hidden; tinted by Slint. Exported from the same VectorCraft master.
- Platform icons are regenerated from `packaging/ondrane.svg` using `cargo xtask app-icons`.

Native source and exports were inspected. Sources use live shapes and text; no photos, image generation, textures or external footage are required. The logo/animation is MIT-licensed; the Inter-derived wordmark retains its font attribution in the third-party notices.
