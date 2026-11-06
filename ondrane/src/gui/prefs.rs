//! Application-side effects of the settings rows: language, skin, video
//! framing, deinterlacing method, audio device, wheel actions, on-screen
//! messages and add-ons.

use super::*;
use crate::options::DEINT_METHODS;

/// Theme tokens a skin file may override (see `ui/theme.slint`).
pub const SKIN_TOKENS: [&str; 20] = [
    "bg-canvas",
    "bg-b1",
    "bg-b2",
    "bg-a1",
    "bg-a2",
    "bg-a3",
    "bg-a4",
    "bg-a5",
    "titlebar",
    "well",
    "thumb-bg",
    "fg-1",
    "fg-2",
    "fg-a1",
    "fg-a2",
    "fg-a3",
    "fg-a4",
    "knob-off",
    "divider-dim",
    "divider-bright",
];

fn color_hex(c: slint::Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.red(), c.green(), c.blue())
}

/// Keys captured by a quick-settings preset ("Setting Preset" in the Figma file).
fn preset_key(k: &str) -> bool {
    [
        "audio.",
        "comp.",
        "sp.",
        "widen.",
        "vf.",
        "crop.",
        "video.zoom",
        "sub.size",
    ]
    .iter()
    .any(|p| k.starts_with(p))
        && k != "audio.device"
}

/// Built-in presets: (name, extra values, picture (brightness, contrast, saturation)).
fn builtin_presets() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "Anime",
            serde_json::json!({
                "extra": {"vf.posterize": false, "audio.widen": true},
                "picture": {"enabled": true, "brightness": 1.05, "contrast": 1.1,
                            "saturation": 1.35, "hue": 0.0, "gamma": 1.0}
            }),
        ),
        (
            "Loud and then Quiet Movies",
            serde_json::json!({
                "extra": {"audio.night": true, "audio.normvol": true, "audio.norm-level": 2.0}
            }),
        ),
    ]
}

impl Controller {
    /// Names of the quick-settings presets (default, built-in, user).
    pub(super) fn preset_names(&self) -> Vec<String> {
        let mut v = vec![tr("Default Preset")];
        v.extend(builtin_presets().iter().map(|p| tr(p.0)));
        if let Some(map) = self
            .st
            .borrow()
            .settings
            .extra
            .get("presets")
            .and_then(|p| p.as_object())
        {
            v.extend(map.keys().cloned());
        }
        v.push(tr("New Preset"));
        v
    }

    fn choose_preset(self: &Rc<Self>, i: usize) {
        let builtins = builtin_presets();
        let user: Vec<(String, serde_json::Value)> = self
            .st
            .borrow()
            .settings
            .extra
            .get("presets")
            .and_then(|p| p.as_object())
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let new_index = 1 + builtins.len() + user.len();
        if i == new_index {
            // snapshot of the current audio / video quick settings
            let snapshot = {
                let st = self.st.borrow();
                let s = &st.settings;
                let extra: serde_json::Map<String, serde_json::Value> = s
                    .extra
                    .iter()
                    .filter(|(k, _)| preset_key(k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                serde_json::json!({
                    "extra": extra,
                    "picture": serde_json::to_value(&s.media.video.picture_adjust).unwrap_or_default(),
                    "eq": {"enabled": s.media.audio.equalizer_enabled, "db": s.media.audio.equalizer_db},
                })
            };
            let name = format!("{} {:02}", tr("Custom Preset"), user.len() + 1);
            let mut st = self.st.borrow_mut();
            let presets = st
                .settings
                .extra
                .entry("presets".to_string())
                .or_insert_with(|| serde_json::json!({}));
            if let Some(m) = presets.as_object_mut() {
                m.insert(name.clone(), snapshot);
            }
            st.settings.set_extra("preset.current", name.clone());
            drop(st);
            self.toast(tr_fmt("Preset saved: {}", &[&name]));
        } else {
            let preset = if i == 0 {
                None
            } else if i <= builtins.len() {
                Some((builtins[i - 1].0.to_string(), builtins[i - 1].1.clone()))
            } else {
                user.get(i - 1 - builtins.len()).cloned()
            };
            {
                let mut st = self.st.borrow_mut();
                let s = &mut st.settings;
                // back to defaults first, then the preset values
                s.extra.retain(|k, _| !preset_key(k));
                s.media.video.picture_adjust = Default::default();
                if let Some((name, v)) = &preset {
                    if let Some(extra) = v.get("extra").and_then(|e| e.as_object()) {
                        for (k, val) in extra {
                            s.extra.insert(k.clone(), val.clone());
                        }
                    }
                    if let Some(p) = v.get("picture") {
                        if let Ok(p) = serde_json::from_value(p.clone()) {
                            s.media.video.picture_adjust = p;
                        }
                    }
                    if let Some(eq) = v.get("eq") {
                        s.media.audio.equalizer_enabled =
                            eq.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);
                        if let Some(db) = eq
                            .get("db")
                            .and_then(|d| serde_json::from_value(d.clone()).ok())
                        {
                            s.media.audio.equalizer_db = db;
                        }
                    }
                    s.set_extra("preset.current", name.clone());
                } else {
                    s.extra.remove("preset.current");
                }
            }
            self.apply_picture();
            self.apply_audio_effects();
            self.sync_eq_ui();
            self.apply_framing();
            self.schedule_apply(crate::options::Apply::Engine);
        }
        self.save_settings();
        self.refresh_quick();
    }

    pub(super) fn settings_ctx(&self) -> pages::Ctx {
        let st = self.st.borrow();
        let page = st.settings_page;
        let searching = !st.settings_search.trim().is_empty();
        drop(st);
        let want = |n: i32| searching || page == n;
        pages::Ctx {
            audio_devices: if want(6) {
                self.player()
                    .map(|p| {
                        p.audio_devices()
                            .into_iter()
                            .map(|d| (d.id, d.description))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            },
            screens: if want(1) {
                self.screen_names()
            } else {
                vec![tr("Current screen")]
            },
            drives: if want(4) {
                extras::optical_drives()
            } else {
                vec![]
            },
            addons: if want(12) {
                crate::addons::list(self.libvlc.get().map(|p| p.as_path()))
            } else {
                vec![]
            },
            remote_url: {
                let s = &self.st.borrow().settings;
                s.ob("remote.web")
                    .then(|| format!("http://{}:{}/", extras::local_ip(), s.oi("remote.port")))
            },
        }
    }

    /// Re-translate everything after "Interface language" changed.
    pub(super) fn apply_language(self: &Rc<Self>) {
        let code = self.st.borrow().settings.app.global.language.clone();
        crate::i18n::set_language(&code);
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_sort_options(model(
            SortKey::ALL.iter().map(|k| ss(tr(k.label()))).collect(),
        ));
        self.refresh_toolbar();
        self.refresh_browse();
        self.refresh_queue();
        self.apply_tray_labels();
    }

    fn apply_tray_labels(self: &Rc<Self>) {
        // the tray menu is built with translated labels: rebuild it
        if self.st.borrow().settings.ob("ui.tray") {
            self.st.borrow_mut().settings.set_extra("ui.tray", false);
            self.apply_tray();
            self.st.borrow_mut().settings.set_extra("ui.tray", true);
            self.apply_tray();
        }
    }

    /// Settings read by the application itself.
    pub(super) fn app_option_changed(self: &Rc<Self>, key: &str) {
        match key {
            "ui.skin" | "ui.skin-file" => self.apply_theme(),
            "ui.decorations" => self.apply_window_prefs(),
            "ui.fs-controls" => {
                let on = self.st.borrow().settings.ob("ui.fs-controls");
                self.ui().global::<AppState>().set_fs_controls(on);
            }
            "ui.tray" => self.apply_tray(),
            "remote.web" | "remote.port" | "remote.password" => {
                // a new password needs a new server
                if key == "remote.password" {
                    if let Some(sv) = self.x.borrow_mut().services.as_mut() {
                        sv.remote = None;
                    }
                }
                self.apply_remote();
            }
            "lastfm.enable" | "lastfm.user" | "lastfm.pass" | "lastfm.url" => {
                self.apply_scrobbler()
            }
            "video.deint-method" => self.apply_deinterlace(),
            "video.scale" | "video.zoom" | "video.position" => self.apply_framing(),
            "audio.device" | "audio.speakers" => self.apply_output_device(),
            "cast.discover" => self.start_cast_discovery(),
            "lib.single" => {
                if self.st.borrow().settings.ob("lib.single") {
                    window::listen_single_instance();
                }
            }
            "lib.use-ml" => self.refresh_browse(),
            k if k.starts_with("crop.") => self.apply_picture(),
            "ui.tree" => self.refresh_queue(),
            _ => {}
        }
    }

    pub(super) fn apply_deinterlace(&self) {
        let Some(p) = self.player() else { return };
        let s = &self.st.borrow().settings;
        let method = DEINT_METHODS[(s.oi("video.deint-method").clamp(0, 10)) as usize].1;
        match s.media.video.deinterlace {
            crate::settings::DeinterlaceMode::Off => p.set_deinterlace(None),
            crate::settings::DeinterlaceMode::On => p.set_deinterlace(Some(method)),
            // automatic: libVLC decides per picture (":deinterlace=-1")
            crate::settings::DeinterlaceMode::Automatic => {}
        }
    }

    pub(super) fn apply_framing(&self) {
        let w = self.ui();
        let a = w.global::<AppState>();
        let s = &self.st.borrow().settings;
        a.set_video_scale(s.ob("video.scale"));
        a.set_video_zoom(s.oi("video.zoom").clamp(0, 5) as i32);
        a.set_video_align(s.oi("video.position").clamp(0, 4) as i32);
        a.set_visualizer(s.oi("audio.visual") > 0);
        a.set_fs_controls(s.ob("ui.fs-controls"));
    }

    /// Theme tokens from the skin file ("Use a custom skin"), or none.
    pub(super) fn apply_skin(&self) {
        let w = self.ui();
        let theme = w.global::<Theme>();
        let (on, file) = {
            let s = &self.st.borrow().settings;
            (s.ob("ui.skin"), s.os("ui.skin-file"))
        };
        let mut colors: HashMap<String, slint::Color> = HashMap::new();
        let st = self.st.borrow();
        if let Some(palette) = st
            .settings
            .app
            .global
            .color_scheme
            .palette(st.settings.app.global.catppuccin_flavor)
        {
            for (key, value) in palette.tokens() {
                if let Some(color) = parse_hex_color(value) {
                    colors.insert(key.into(), color);
                }
            }
        }
        drop(st);
        if on && !file.trim().is_empty() {
            let path = library::expand_tilde(file.trim());
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())
                }) {
                Ok(v) => {
                    if let Some(map) = v.get("colors").and_then(|c| c.as_object()) {
                        for (k, val) in map {
                            if let Some(c) = val.as_str().and_then(parse_hex_color) {
                                colors.insert(k.clone(), c);
                            }
                        }
                    }
                    if let Some(c) = v
                        .get("accent")
                        .and_then(|c| c.as_str())
                        .and_then(parse_hex_color)
                    {
                        theme.set_accent(c);
                    }
                    if let Some(d) = v.get("dark").and_then(|d| d.as_bool()) {
                        theme.set_dark(d);
                    }
                }
                Err(e) => self.show_error(&tr("Skin file"), &e),
            }
        }
        let accent = theme.get_accent();
        let light =
            crate::themes::accent_uses_light_ink(accent.red(), accent.green(), accent.blue());
        theme.set_on_accent(if light {
            slint::Color::from_rgb_u8(255, 255, 255)
        } else {
            slint::Color::from_rgb_u8(0, 0, 0)
        });
        let get = |k: &str| {
            colors
                .get(k)
                .copied()
                .unwrap_or(slint::Color::from_argb_u8(0, 0, 0, 0))
        };
        theme.set_o_bg_canvas(get("bg-canvas"));
        theme.set_o_bg_b1(get("bg-b1"));
        theme.set_o_bg_b2(get("bg-b2"));
        theme.set_o_bg_a1(get("bg-a1"));
        theme.set_o_bg_a2(get("bg-a2"));
        theme.set_o_bg_a3(get("bg-a3"));
        theme.set_o_bg_a4(get("bg-a4"));
        theme.set_o_bg_a5(get("bg-a5"));
        theme.set_o_titlebar(get("titlebar"));
        theme.set_o_well(get("well"));
        theme.set_o_thumb_bg(get("thumb-bg"));
        theme.set_o_fg_1(get("fg-1"));
        theme.set_o_fg_2(get("fg-2"));
        theme.set_o_fg_a1(get("fg-a1"));
        theme.set_o_fg_a2(get("fg-a2"));
        theme.set_o_fg_a3(get("fg-a3"));
        theme.set_o_fg_a4(get("fg-a4"));
        theme.set_o_knob_off(get("knob-off"));
        theme.set_o_divider_dim(get("divider-dim"));
        theme.set_o_divider_bright(get("divider-bright"));
    }

    /// "Export the current theme as a skin".
    pub(super) fn export_skin(&self) {
        let w = self.ui();
        let t = w.global::<Theme>();
        let values = [
            t.get_bg_canvas(),
            t.get_bg_b1(),
            t.get_bg_b2(),
            t.get_bg_a1(),
            t.get_bg_a2(),
            t.get_bg_a3(),
            t.get_bg_a4(),
            t.get_bg_a5(),
            t.get_titlebar(),
            t.get_well(),
            t.get_thumb_bg(),
            t.get_fg_1(),
            t.get_fg_2(),
            t.get_fg_a1(),
            t.get_fg_a2(),
            t.get_fg_a3(),
            t.get_fg_a4(),
            t.get_knob_off(),
            t.get_divider_dim(),
            t.get_divider_bright(),
        ];
        let mut colors = serde_json::Map::new();
        for (k, c) in SKIN_TOKENS.iter().zip(values) {
            colors.insert(k.to_string(), color_hex(c).into());
        }
        let json = serde_json::json!({
            "name": "My Ondrane skin",
            "dark": t.get_dark(),
            "accent": color_hex(t.get_accent()),
            "colors": colors,
        });
        let file = OndraneSettings::config_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .join("skin.json");
        match std::fs::write(
            &file,
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        ) {
            Ok(()) => {
                {
                    let mut st = self.st.borrow_mut();
                    st.settings.set_extra("ui.skin-file", library::tilde(&file));
                }
                self.save_settings();
                self.refresh_settings();
                self.toast(tr_fmt("Skin saved to {}", &[&library::tilde(&file)]));
            }
            Err(e) => self.show_error(&tr("Skin file"), &e.to_string()),
        }
    }

    /// Mouse wheel over the video: volume, seek or nothing.
    pub(super) fn wheel(self: &Rc<Self>, dir: i32, horizontal: bool) {
        let (action, reverse, step) = {
            let s = &self.st.borrow().settings;
            let a = if horizontal {
                [1, 0, 2][s.oi("mouse.hwheel").clamp(0, 2) as usize]
            } else {
                s.oi("mouse.wheel").clamp(0, 2)
            };
            (
                a,
                s.app.controls.reverse_scroll,
                s.media.audio.volume_step as i32,
            )
        };
        let dir = if reverse { -dir } else { dir };
        match action {
            0 => {
                let w = self.ui();
                let a = w.global::<AppState>();
                self.set_volume(a.get_volume() + dir * step);
                self.osd(tr_fmt("Volume {}%", &[&a.get_volume().to_string()]));
            }
            1 => {
                let secs = self.st.borrow().settings.app.controls.skip_very_short_s as i64;
                self.jump(dir as i64 * secs);
            }
            _ => {}
        }
    }

    /// On-screen feedback of a shortcut ("Show feedback for shortcuts").
    pub fn osd(&self, text: impl AsRef<str>) {
        let (on, secs) = {
            let s = &self.st.borrow().settings;
            (s.ob("osd.feedback"), s.of("osd.feedback-dur"))
        };
        if on {
            self.toast_for(text, secs);
        }
    }

    pub fn toast_for(&self, text: impl AsRef<str>, secs: f64) {
        let w = self.ui();
        let a = w.global::<AppState>();
        a.set_toast(ss(text));
        a.set_toast_visible(true);
        self.st.borrow_mut().toast_until =
            Some(Instant::now() + Duration::from_millis((secs.clamp(0.3, 30.0) * 1000.0) as u64));
    }

    pub(super) fn addon_command(self: &Rc<Self>, head: &str, arg: &str) {
        let Some((flag, path)) = arg.split_once(':') else {
            return;
        };
        let list = crate::addons::list(self.libvlc.get().map(|p| p.as_path()));
        let Some(addon) = list.iter().find(|a| a.path.to_string_lossy() == path) else {
            return;
        };
        let res = if head == "addon-remove" {
            crate::addons::remove(addon).map(|_| ())
        } else {
            crate::addons::set_enabled(addon, flag == "true").map(|_| ())
        };
        match res {
            Ok(()) => self.schedule_apply(crate::options::Apply::Engine),
            Err(e) => self.show_error(&tr("Add-ons"), &e.to_string()),
        }
        self.refresh_settings();
    }

    /// Commands raised by settings rows that need the controller.
    pub(super) fn settings_command(self: &Rc<Self>, head: &str, arg: &str) -> bool {
        match head {
            "drive-index" => {
                let i: usize = arg.parse().unwrap_or(0);
                let drive = if i == 0 {
                    String::new()
                } else {
                    let mut drives = extras::optical_drives();
                    let cur = self.st.borrow().settings.os("play.drive");
                    if !cur.is_empty() && !drives.contains(&cur) {
                        drives.push(cur);
                    }
                    drives.get(i - 1).cloned().unwrap_or_default()
                };
                self.st.borrow_mut().settings.set_extra("play.drive", drive);
            }
            "audio-device-index" => {
                let i: usize = arg.parse().unwrap_or(0);
                let id = if i == 0 {
                    String::new()
                } else {
                    self.player()
                        .and_then(|p| p.audio_devices().get(i - 1).map(|d| d.id.clone()))
                        .unwrap_or_default()
                };
                self.st.borrow_mut().settings.set_extra("audio.device", id);
                self.apply_output_device();
            }
            "open-eq" => {
                self.ui().global::<AppState>().set_settings_open(false);
                self.handle_act_int("quick", 2);
            }
            "settings-then" => {
                self.ui().global::<AppState>().set_settings_open(false);
                self.command(arg);
            }
            "skin-export" => self.export_skin(),
            "preset" => self.choose_preset(arg.parse().unwrap_or(0)),
            _ => return false,
        }
        self.save_settings();
        true
    }
}
