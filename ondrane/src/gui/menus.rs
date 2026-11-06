//! Menu contents: Figma "right-click menu", "menu" (main), "context menu"
//! (add), "Sorting Options", "View Options" and the file / folder / playlist
//! context menus.

use super::*;

struct B<'a> {
    c: &'a Rc<Controller>,
    out: Vec<MenuEntry>,
    sep: bool,
}

impl<'a> B<'a> {
    fn new(c: &'a Rc<Controller>) -> Self {
        Self {
            c,
            out: Vec::new(),
            sep: false,
        }
    }

    fn shortcut(&self, cmd: &str) -> String {
        let st = self.c.st.borrow();
        keys::key_of(cmd, &st.settings.app.controls.shortcuts)
            .replace('+', " + ")
            .replace("  +  ", " + + ")
    }

    fn push(&mut self, id: &str, icon: &str, label: &str, shortcut: &str) -> &mut MenuEntry {
        let entry = MenuEntry {
            id: ss(id),
            has_icon: !icon.is_empty(),
            icon: if icon.is_empty() {
                IconData::default()
            } else {
                self.c.icon(icon)
            },
            label: ss(tr(label)),
            shortcut: ss(shortcut),
            separator: std::mem::take(&mut self.sep),
            enabled: true,
            ..Default::default()
        };
        self.out.push(entry);
        self.out.last_mut().unwrap()
    }

    /// Entry whose shortcut comes from the key table.
    fn cmd(&mut self, id: &str, icon: &str, label: &str) -> &mut MenuEntry {
        let s = self.shortcut(id.split(':').next().unwrap_or(id));
        let s = if keys::ACTIONS.iter().any(|a| a.1 == id) {
            s
        } else {
            String::new()
        };
        self.push(id, icon, label, &s)
    }

    fn sub(&mut self, id: &str, icon: &str, label: &str) {
        self.push(id, icon, label, "").submenu = true;
    }

    fn check(&mut self, id: &str, label: &str, checked: bool) {
        self.push(id, "", label, "").checked = checked;
    }

    fn separator(&mut self) {
        self.sep = !self.out.is_empty();
    }
}

pub fn build(c: &Rc<Controller>, id: &str) -> (Vec<MenuEntry>, f32) {
    let mut b = B::new(c);
    let playing = c.ui().global::<AppState>().get_playing();
    let width = match id {
        "context" => {
            b.cmd("play-pause", "play", if playing { "Pause" } else { "Play" })
                .accent_icon = true;
            b.cmd("stop", "stop", "Stop").accent_icon = true;
            b.cmd("prev", "previous", "Previous Item").accent_icon = true;
            b.cmd("next", "next", "Next Item").accent_icon = true;
            b.cmd("snapshot", "snapshot", "Snapshot").accent_icon = true;
            b.separator();
            b.cmd("hide-ui", "ui", "Hide UI");
            b.separator();
            b.sub("sub-playback", "playback-2t", "Playback");
            b.sub("sub-video", "video-2t", "Video");
            b.sub("sub-audio", "audio-2t", "Audio");
            b.sub("sub-subtitle", "subtitles-2t", "Subtitle");
            b.separator();
            b.sub("sub-more", "settings", "More Settings");
            b.sub("sub-playlist", "playlist-2t", "Playlist");
            b.sub("sub-open", "file-2t", "Open Media");
            b.cmd("quit", "quit", "Quit Ondrane");
            292.0
        }
        "main" | "app" => {
            b.cmd("open-files", "file-2t", "Open File(s)");
            b.cmd("open-folder", "folder", "Open Folder");
            b.cmd("open-network", "streaming", "Open Network Stream");
            b.push("open-disc", "disc-2t", "Open Disc", "");
            b.push(
                "open-capture",
                "capture-device-2t",
                "Open Capture Device",
                "",
            );
            b.push("convert", "swap", "Convert", "");
            b.sub("sub-recent", "recent", "Open Recent");
            b.separator();
            let profile = c.st.borrow().toolbar.profile.label();
            b.sub(
                "sub-profile",
                "profile",
                &crate::i18n::tr_fmt("Profile ({})", &[&tr(profile)]),
            );
            b.cmd("shortcuts", "hotkeys", "Keyboard Shortcuts");
            b.push("customize", "paint", "Customize Player", "");
            b.cmd("settings", "settings", "Settings");
            b.cmd("about", "info", "About Ondrane");
            375.0
        }
        "add" => {
            b.push("enqueue-files", "add", "Add File", "");
            b.push("add-folder", "new-folder", "Add Folder", "");
            b.cmd("open-network", "streaming", "Open Network Stream");
            b.push("new-playlist", "playlist", "New Playlist", "");
            265.0
        }
        "cast" => {
            for (id, label, checked) in c.cast_entries() {
                b.check(&id, &label, checked);
            }
            if b.out.len() == 1 {
                let none = if c.st.borrow().settings.ob("cast.discover") {
                    "Searching for cast devices…"
                } else {
                    "Device discovery is off (Settings › Recording)"
                };
                b.push("none", "", none, "").enabled = false;
            }
            300.0
        }
        "sort" => {
            let cur = c.st.borrow().sort;
            for (i, k) in SortKey::ALL.iter().enumerate() {
                b.check(&format!("sort:{i}"), k.label(), *k == cur);
            }
            265.0
        }
        "view" => {
            let grid = c.ui().global::<AppState>().get_grid();
            b.push("view-grid", "grid", "Grid View", "").checked = grid;
            b.push("view-list", "list", "List View", "").checked = !grid;
            265.0
        }
        "browse-more" => {
            b.push("play-all", "play", "Play All", "");
            b.push("shuffle-all", "shuffle-right", "Shuffle All", "");
            b.push("enqueue-all", "queue", "Add All to Queue", "");
            b.push(
                "mark-all-watched",
                "check-active",
                "Mark All as Watched",
                "",
            );
            b.separator();
            b.push("add-subtitles", "subtitles", "Add Subtitles to Folder", "");
            b.push("reveal", "folder", "Show in File Explorer", "");
            b.push("refresh", "refresh", "Refresh", "");
            265.0
        }
        "item-file" => {
            let row =
                c.st.borrow()
                    .menu_item
                    .and_then(|i| c.st.borrow().rows.get(i).cloned());
            let Some(row) = row else { return (vec![], 0.0) };
            let watched = c.st.borrow().store.is_watched(&row.target);
            let fav = c.st.borrow().store.is_favorite(&row.target);
            b.push("item:play", "playback", "Play", "");
            b.push(
                "item:watched",
                if watched {
                    "check-selected"
                } else {
                    "check-active"
                },
                if watched {
                    "Mark as Unwatched"
                } else {
                    "Mark as Watched"
                },
                "",
            );
            b.push(
                "item:favorite",
                if fav { "favorite-active" } else { "favorite" },
                if fav { "Unfavorite" } else { "Favorite" },
                "",
            );
            b.push("item:queue", "queue", "Add to Queue", "");
            b.sub("sub-add-to-playlist", "playlist", "Add to Playlist");
            if row.kind == RowKind::Media && !library::is_url(&row.target) {
                b.push("item:reveal", "folder", "Show in File Explorer", "");
                b.push("item:subtitles", "subtitles", "Add Subtitle Tracks", "");
            }
            if matches!(c.st.borrow().loc, Location::Playlist(_)) {
                b.push(
                    "item:remove-from-playlist",
                    "remove",
                    "Remove from Playlist",
                    "",
                );
            }
            b.push("item:info", "info", "Properties & Metadata", "");
            265.0
        }
        "item-folder" => {
            b.push("item:play", "playback", "Open", "");
            b.push("item:shuffle", "shuffle-right", "Shuffle", "");
            b.push("item:queue", "queue", "Add to Queue", "");
            b.push("item:reveal", "folder", "Show in File Explorer", "");
            265.0
        }
        "item-playlist" => {
            b.push("item:play", "playback", "Open", "");
            b.push("item:shuffle", "shuffle-right", "Shuffle", "");
            b.push("item:export", "save", "Export Playlist", "");
            b.push("item:queue", "queue", "Add to Queue", "");
            b.push("item:delete-playlist", "remove", "Delete Playlist", "");
            265.0
        }
        _ => 0.0,
    };
    (b.out, width)
}

pub fn build_sub(c: &Rc<Controller>, id: &str) -> Vec<MenuEntry> {
    let mut b = B::new(c);
    let w = c.ui();
    let a = w.global::<AppState>();
    match id {
        "sub-playback" => {
            let rate = a.get_rate();
            for s in SPEEDS {
                b.check(
                    &format!("speed:{s}"),
                    &format!("{s}x"),
                    (rate - s).abs() < 0.01,
                );
            }
            b.separator();
            b.cmd("jump:-short", "backward-10-sec", "Jump Backward");
            b.cmd("jump:+short", "forward-10-sec", "Jump Forward");
            b.separator();
            let r = c.st.borrow().repeat;
            b.check("repeat:0", "Repeat Off", r == 0);
            b.check("repeat:1", "Repeat All", r == 1);
            b.check("repeat:2", "Repeat One", r == 2);
            b.check("shuffle", "Shuffle", c.st.borrow().shuffle);
            b.separator();
            let rec = c.x.borrow().recording.is_some();
            let streaming = c.x.borrow().streaming.is_some();
            b.push(
                "record",
                if rec { "record-active" } else { "record" },
                if rec { "Stop Recording" } else { "Record" },
                "",
            );
            b.push(
                "stream-dialog",
                if streaming {
                    "streaming-active"
                } else {
                    "streaming"
                },
                "Stream Out…",
                "",
            );
            b.push("cast-menu", "tv", "Cast…", "");
        }
        "sub-video" => {
            b.cmd("fullscreen", "fullscreen", "Fullscreen");
            b.cmd("snapshot", "snapshot", "Take Snapshot");
            b.separator();
            let cur = c.st.borrow().aspect_idx;
            for (i, (label, _)) in ASPECTS.iter().enumerate() {
                b.check(
                    &format!("aspect:{i}"),
                    &crate::i18n::tr_fmt("Aspect {}", &[&tr(label)]),
                    i == cur,
                );
            }
            b.separator();
            let crop = c.st.borrow().crop_idx;
            for (i, (label, _)) in CROPS.iter().enumerate() {
                b.check(
                    &format!("crop:{i}"),
                    &crate::i18n::tr_fmt("Crop {}", &[&tr(label)]),
                    i == crop,
                );
            }
        }
        "sub-audio" => {
            if let Some(p) = c.player() {
                let cur = p.current_audio_track();
                for t in p.audio_tracks() {
                    b.check(&format!("audio-track:{}", t.id), &t.name, t.id == cur);
                }
            }
            b.separator();
            b.cmd(
                "mute",
                "close-none",
                if a.get_muted() { "Unmute" } else { "Mute" },
            );
            b.push("quick-audio", "audio", "Equalizer & Effects", "");
        }
        "sub-subtitle" => {
            if let Some(p) = c.player() {
                let cur = p.current_subtitle_track();
                for t in p.subtitle_tracks() {
                    b.check(&format!("subtitle-track:{}", t.id), &t.name, t.id == cur);
                }
            }
            b.separator();
            b.push("load-subtitle", "subtitle", "Load Subtitle File…", "");
            b.push("add-subtitles", "subtitles", "Add Subtitles to Folder…", "");
            b.cmd("sub-delay:-", "", "Delay −50 ms");
            b.cmd("sub-delay:+", "", "Delay +50 ms");
        }
        "sub-more" => {
            b.push("customize", "paint", "Customize Player", "");
            b.cmd("shortcuts", "hotkeys", "Keyboard Shortcuts");
            b.cmd("settings", "settings", "Settings");
            b.cmd("about", "info", "About Ondrane");
        }
        "sub-playlist" => {
            b.push("toggle-queue", "lists", "Show Queue", "");
            b.push("save-queue", "save", "Save Queue as Playlist", "");
            b.push("clear-queue", "remove", "Clear Queue", "");
            b.separator();
            b.push("playlists", "playlist", "All Playlists", "");
        }
        "sub-open" => {
            b.cmd("open-files", "file", "Open File(s)");
            b.cmd("open-folder", "folder", "Open Folder");
            b.cmd("open-network", "streaming", "Open Network Stream");
            b.push("open-disc", "disc", "Open Disc", "");
            b.push("open-capture", "capture-device", "Open Capture Device", "");
            b.push("convert", "swap", "Convert", "");
        }
        "sub-recent" => {
            let hist: Vec<String> =
                c.st.borrow()
                    .store
                    .history
                    .iter()
                    .take(12)
                    .map(|h| h.target.clone())
                    .collect();
            for t in hist {
                b.push(
                    &format!("open-recent:{t}"),
                    "",
                    &library::display_name(&t),
                    "",
                );
            }
            if b.out.is_empty() {
                b.push("none", "", "No recent media", "").enabled = false;
            } else {
                b.separator();
                b.push("clear-history", "remove", "Clear Recent", "");
            }
        }
        "sub-profile" => {
            let cur = c.st.borrow().toolbar.profile;
            for p in Profile::PRESETS {
                b.check(&format!("profile:{}", p.label()), p.label(), p == cur);
            }
            b.separator();
            b.push("customize", "add", "New", "");
        }
        "sub-add-to-playlist" => {
            b.push("add-to-playlist:new", "add", "New Playlist", "");
            let names: Vec<String> =
                c.st.borrow()
                    .store
                    .playlists
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
            for (i, n) in names.iter().enumerate() {
                b.push(&format!("add-to-playlist:{i}"), "playlist", n, "");
            }
        }
        _ => {}
    }
    b.out
}
