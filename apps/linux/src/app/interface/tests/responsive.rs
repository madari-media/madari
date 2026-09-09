use super::*;
use std::time::{Duration, Instant};

fn settle() {
    let until = Instant::now() + Duration::from_millis(120);
    while Instant::now() < until {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn fits(ui: &Ui, page: &str, width: i32) {
    settle();
    assert!(
        ui.window.width() >= width - 20,
        "{page}: test window did not reach requested width {width}: {}",
        ui.window.width()
    );
    assert!(
        ui.window.width() <= width,
        "{page}: window expanded to {} from {width}",
        ui.window.width()
    );
    let minimum = ui.content.measure(gtk::Orientation::Horizontal, -1).0;
    assert!(
        minimum <= ui.window.width(),
        "{page}: minimum content width {minimum} exceeds {width}"
    );
}

fn find_widget(
    root: &gtk::Widget,
    predicate: &impl Fn(&gtk::Widget) -> bool,
) -> Option<gtk::Widget> {
    if predicate(root) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(found) = find_widget(&widget, predicate) {
            return Some(found);
        }
    }
    None
}

fn capture(window: &adw::ApplicationWindow, name: &str) {
    let Some(directory) = std::env::var_os("MADARI_LAYOUT_SNAPSHOTS") else {
        return;
    };
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(window)).snapshot(
        &snapshot,
        window.width() as f64,
        window.height() as f64,
    );
    if let Some(node) = snapshot.to_node() {
        window
            .renderer()
            .unwrap()
            .render_texture(&node, None)
            .save_to_png(PathBuf::from(directory).join(format!("{name}.png")))
            .unwrap();
    }
}

#[test]
#[ignore = "Requires a display; run with xvfb-run and --ignored"]
fn all_screen_layouts_fit_small_and_large_windows() {
    adw::init().unwrap();
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.add_css_class("continue-section");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let first = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let second = gtk::Box::new(gtk::Orientation::Vertical, 0);
    row.append(&first);
    row.append(&second);
    section.append(&row);
    crate::app::browsing::episodes::hide_continue_card(&first);
    assert!(!first.is_visible() && second.is_visible() && section.is_visible());
    crate::app::browsing::episodes::hide_continue_card(&second);
    assert!(!section.is_visible());
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let app = adw::Application::builder()
        .application_id("media.madari.LayoutTest")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gtk::gio::Cancellable::NONE).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(Runtime::new().unwrap());
    let profiles = runtime
        .block_on(Profiles::open(temp.path().join("profiles.sqlite")))
        .unwrap();
    let profile = runtime
        .block_on(profiles.create(
            None,
            "A long profile name for layout testing".into(),
            false,
            String::new(),
        ))
        .unwrap();
    let session = runtime
        .block_on(profiles.unlock(profile.id, String::new()))
        .unwrap();
    let ui = build_ui(
        &app,
        runtime.clone(),
        profiles.clone(),
        temp.path().join("artwork"),
    );
    let core = profiles.core(session.clone());
    *ui.session.borrow_mut() = Some(session);
    *ui.core.borrow_mut() = Some(core.clone());
    let snapshot = runtime.block_on(core.snapshot()).unwrap();
    let meta: madari_model::Meta = serde_json::from_value(serde_json::json!({
        "id":"show", "type":"series", "name":"A very long series title with enough words to wrap on a small screen",
        "description":"An extended description of this series that should wrap neatly without pushing any controls beyond the edge of the screen.",
        "cast":["An actor with a very long name", "Another actor"], "director":["A director with a long name"],
        "videos":[{"id":"show:1:1", "title":"An episode with a long descriptive name", "season":1, "episode":1}]
    })).unwrap();
    let key = madari_model::ItemKey {
        installation_id: "fixture".into(),
        content_type: "series".into(),
        item_id: "show".into(),
    };
    for (width, height) in [
        (360, 640),
        (480, 320),
        (600, 640),
        (800, 600),
        (1100, 740),
        (360, 640),
    ] {
        ui.window.set_default_size(width, height);
        ui.window.present();
        settle();
        ui.picker();
        fits(&ui, "profile picker", width);
        let profiles = [
            ("Omkar", false, false),
            ("Cinema", false, true),
            ("Kids", true, false),
            ("A profile with a long name", false, false),
        ]
        .into_iter()
        .map(|(name, kids, pin_protected)| {
            let mut profile = ui.session.borrow().as_ref().unwrap().profile.clone();
            profile.name = name.into();
            profile.kids = kids;
            profile.pin_protected = pin_protected;
            profile
        })
        .collect();
        ui.profile_picker(profiles);
        fits(&ui, "multiple profiles", width);
        capture(&ui.window, &format!("profiles-{width}"));

        ui.build_navigation();
        ui.search_page(snapshot.clone());
        fits(&ui, "search", width);
        ui.clear();
        let trakt_holder = form();
        ui.content.append(&trakt_holder);
        let title = madari_native::trakt::Title {
            trakt_id: 1,
            imdb: Some("tt1".into()),
            tmdb: None,
            name: "A long imported Trakt title that should remain inside the horizontal rail"
                .into(),
            content_type: "series".into(),
            year: Some(2024),
            season: Some(1),
            episode: Some(2),
            watched_at: None,
            details: Default::default(),
        };
        ui.trakt_collection(
            &trakt_holder,
            madari_native::trakt::Data {
                username: "a-very-long-trakt-account-name".into(),
                watchlist: vec![title.clone(); 65],
                history: vec![title.clone()],
                lists: vec![madari_native::trakt::List {
                    id: 1,
                    name: "A personal Trakt list with a very long name for small screens".into(),
                    items: vec![title],
                }],
                ..Default::default()
            },
            snapshot.clone(),
        );
        fits(&ui, "Trakt lists", width);
        capture(&ui.window, &format!("trakt-{width}"));
        let more = find_widget(&ui.content.clone().upcast(), &|w| {
            w.downcast_ref::<gtk::Button>()
                .is_some_and(|b| b.label().as_deref() == Some("Show more"))
        })
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
        assert!(more.is_visible());
        more.emit_clicked();
        more.emit_clicked();
        assert!(!more.is_visible());
        ui.search_page(snapshot.clone());

        let (scroll, row) = crate::app::browsing::home::rail();
        for _ in 0..30 {
            row.append(&ui.poster(key.clone(), meta.clone()));
        }
        ui.content.append(&scroll);
        fits(&ui, "long search result row", width);
        let first = row.first_child().unwrap();
        let last = row.last_child().unwrap();
        assert_eq!(
            first.compute_bounds(&row).unwrap().y(),
            last.compute_bounds(&row).unwrap().y()
        );
        let adjustment = scroll.hadjustment();
        assert!(adjustment.upper() > adjustment.page_size());
        adjustment.set_value(adjustment.upper() - adjustment.page_size());
        assert!(adjustment.value() > 0.0);
        ui.clear();
        for state in ["Downloading", "Seeding", "Paused"] {
            let (row, _, _) = crate::app::transfers::TransferRow::new(
                "A very long movie title with season and episode details · 2160p HEVC",
            );
            row.update(&madari_native::internal_media::TorrentStats {
                state: state.into(),
                downloaded: 2147483648,
                total: 4294967296,
                download_bytes_per_second: 10485760,
                upload_bytes_per_second: 524288,
                peers: 24,
            });
            ui.content.append(&row.root);
        }
        fits(&ui, "torrent rows", width);
        capture(&ui.window, &format!("torrents-{width}"));
        ui.detail_view(key.clone(), meta.clone(), false, vec![]);
        fits(&ui, "details and episodes", width);
        capture(&ui.window, &format!("details-{width}"));
        ui.clear();
        let holder = form();
        ui.continue_card(&holder, key.clone(), meta.clone(), &[]);
        ui.content.append(&holder);
        fits(&ui, "continue watching", width);
        ui.clear();
        let grid = gtk::FlowBox::builder()
            .min_children_per_line(1)
            .max_children_per_line(12)
            .build();
        for _ in 0..4 {
            grid.insert(&ui.poster(key.clone(), meta.clone()), -1);
        }
        ui.content.append(&grid);
        fits(&ui, "poster grid", width);
        ui.loading_page(true);
        fits(&ui, "loading", width);
        ui.page_loading.set(false);
        ui.clear();
        let columns = responsive::row(28);
        let calendar = gtk::Calendar::new();
        calendar.add_css_class("release-calendar");
        columns.append(&calendar);
        columns.append(&label(
            "An agenda with long episode names and release information",
            "body",
        ));
        ui.content.append(&columns);
        fits(&ui, "calendar", width);
        assert_eq!(
            columns.orientation(),
            if width <= 600 {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            }
        );
        ui.clear();
        let stream = serde_json::from_value(serde_json::json!({"name":"Provider name\n4K HDR Dolby Vision", "title":"A long release filename with many quality and language details", "url":"https://example.com/video"})).unwrap();
        let button = gtk::Button::new();
        button.add_css_class("source-card");
        button.set_child(Some(&crate::app::browsing::stream_list::content(
            &stream, false,
        )));
        ui.content.append(&button);
        fits(&ui, "source picker", width);
        let first = gtk::Button::with_label("First addon source");
        let second = gtk::Button::with_label("Second addon source");
        let filter = crate::app::browsing::stream_list::addon_filter(vec![
            (
                "addon-a".into(),
                "An addon with a very long display name".into(),
                2,
                first.clone().upcast(),
            ),
            (
                "addon-b".into(),
                "An addon with a very long display name".into(),
                1,
                second.clone().upcast(),
            ),
        ]);
        ui.content.prepend(&filter);
        ui.content.append(&first);
        ui.content.append(&second);
        let tabs = find_widget(filter.upcast_ref(), &|widget| {
            widget.has_css_class("addon-tabs")
        })
        .unwrap();
        let all = tabs
            .first_child()
            .unwrap()
            .downcast::<gtk::ToggleButton>()
            .unwrap();
        let addon_a = all
            .next_sibling()
            .unwrap()
            .downcast::<gtk::ToggleButton>()
            .unwrap();
        let addon_b = addon_a
            .next_sibling()
            .unwrap()
            .downcast::<gtk::ToggleButton>()
            .unwrap();
        addon_b.set_active(true);
        assert!(!first.is_visible() && second.is_visible());
        addon_a.set_active(true);
        assert!(first.is_visible() && !second.is_visible());
        all.set_active(true);
        assert!(first.is_visible() && second.is_visible());
        fits(&ui, "addon stream filter", width);
        ui.saved_page(snapshot.clone());
        fits(&ui, "library", width);
    }
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let profile = ui.session.borrow().as_ref().unwrap().profile.clone();
    ui.profile_picker(vec![profile]);
    for _ in 0..4 {
        settle();
    }
    let grid = find_widget(ui.content.upcast_ref(), &|widget| {
        widget.has_css_class("profile-grid")
    })
    .unwrap();
    let tile = grid
        .first_child()
        .unwrap()
        .downcast::<gtk::FlowBoxChild>()
        .unwrap();
    let reveal = tile.child().unwrap().downcast::<gtk::Revealer>().unwrap();
    assert!(
        reveal.reveals_child() && reveal.is_child_revealed(),
        "profile entrance animation did not finish"
    );
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    ui.preferences_view(snapshot);
    settle();
    let settings = ui.preferences.borrow().as_ref().unwrap().clone();
    assert!(
        settings.width() <= 360,
        "settings width {}",
        settings.width()
    );
    for page in ui.preferences_pages.borrow().iter() {
        settings.set_visible_page(page);
        settle();
        assert!(
            page.measure(gtk::Orientation::Horizontal, -1).0 <= settings.width(),
            "settings page {:?} overflows",
            page.title()
        );
    }
    settings.destroy();
    ui.preferences.borrow_mut().take();
    ui.window.set_default_size(360, 320);
    settle();
    let fields = form();
    for _ in 0..8 {
        fields.append(&entry("A setting value", false));
    }
    ui.dialog(
        "Settings dialog",
        "A form that must remain usable in a short window.",
        &fields,
        "Save",
        |_| {},
    );
    settle();
    let dialog = ui.window.visible_dialog().unwrap();
    assert!(dialog.width() <= ui.window.width() && dialog.height() <= ui.window.height());
    dialog.close();
    settle();
    let controls = crate::app::playback::controls::Controls::new();
    controls.trakt_status.set_text("Trakt · Watched synced");
    controls.trakt_status.set_visible(true);
    let chapters = find_widget(controls.root.upcast_ref(), &|widget| {
        widget.tooltip_text().as_deref() == Some("Chapters")
    })
    .unwrap();
    assert!(!chapters.is_visible());
    let sample_chapters = [crate::app::playback::engine::Chapter {
        index: 0,
        title: "Opening".into(),
        start_ms: Some(0),
    }];
    controls.refresh_chapters(&sample_chapters);
    assert!(chapters.is_visible());
    controls.refresh_chapters(&[]);
    assert!(!chapters.is_visible());
    controls.refresh_chapters(&sample_chapters);
    controls.set_title("A very long title with season and episode details for compact playback");
    controls.loaded();
    controls.episode_neighbors(meta.videos.first(), meta.videos.first());
    controls.set_episodes_available(true);
    let transport = find_widget(controls.root.upcast_ref(), &|widget| {
        widget.has_css_class("player-transport")
    })
    .unwrap();
    let mut child = transport.first_child();
    let mut icons = Vec::new();
    while let Some(widget) = child {
        child = widget.next_sibling();
        icons.push(
            widget
                .downcast::<gtk::Button>()
                .unwrap()
                .icon_name()
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(
        icons,
        [
            "media-skip-backward-symbolic",
            "media-seek-backward-symbolic",
            "media-playback-pause-symbolic",
            "media-seek-forward-symbolic",
            "media-skip-forward-symbolic"
        ]
    );

    ui.window.set_content(Some(&controls.root));
    let settings = find_widget(controls.root.upcast_ref(), &|widget| {
        widget
            .downcast_ref::<gtk::MenuButton>()
            .is_some_and(|button| button.tooltip_text().as_deref() == Some("Player settings"))
    })
    .unwrap()
    .downcast::<gtk::MenuButton>()
    .unwrap();
    for (width, height) in [(360, 640), (480, 280), (1100, 740), (360, 640)] {
        ui.window.set_default_size(width, height);
        settle();
        controls.tick();
        settle();
        let bottom = find_widget(controls.root.upcast_ref(), &|widget| {
            widget.has_css_class("player-bottom")
        })
        .unwrap();
        assert!(
            bottom.measure(gtk::Orientation::Horizontal, -1).0 <= width,
            "player controls overflow at {width}"
        );
        let transport_bounds = transport.compute_bounds(&controls.root).unwrap();
        assert!(
            (transport_bounds.x() + transport_bounds.width() / 2.0
                - controls.root.width() as f32 / 2.0)
                .abs()
                <= 2.0,
            "transport is not centered at {width}"
        );
        let skip = gtk::Button::with_label("Skip intro");
        skip.add_css_class("skip-intro");
        controls.place_skip_action(&skip);
        controls.root.add_overlay(&skip);
        settle();
        let bounds = skip.compute_bounds(&controls.root).unwrap();
        let controls_bounds = bottom.compute_bounds(&controls.root).unwrap();
        assert!(bounds.x() >= 0.0 && bounds.x() + bounds.width() <= controls.root.width() as f32);
        assert!(
            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= controls_bounds.y(),
            "skip action overlaps controls at {width}x{height}"
        );
        capture(&ui.window, &format!("player-skip-{width}"));
        controls.root.remove_overlay(&skip);
        if width == 360 {
            let volume = find_widget(controls.root.upcast_ref(), &|widget| {
                widget.has_css_class("inline-volume")
            })
            .unwrap();
            let controllers = volume.observe_controllers();
            let motion = (0..controllers.n_items())
                .filter_map(|index| {
                    controllers
                        .item(index)?
                        .downcast::<gtk::EventControllerMotion>()
                        .ok()
                })
                .next()
                .unwrap();
            motion.emit_by_name::<()>("enter", &[&0.0f64, &0.0f64]);
            settle();
            let reveal = find_widget(&volume, &|widget| widget.has_css_class("volume-slider"))
                .unwrap()
                .downcast::<gtk::Revealer>()
                .unwrap();
            assert!(
                reveal.reveals_child(),
                "volume did not expand inline on hover"
            );
            assert!(
                bottom.measure(gtk::Orientation::Horizontal, -1).0 <= width,
                "expanded volume overflowed the player"
            );
            motion.emit_by_name::<()>("leave", &[]);
            for _ in 0..3 {
                settle();
            }
            assert!(
                !reveal.reveals_child(),
                "volume did not collapse after hover"
            );
        }
        settings.popup();
        settle();
        let popover = settings.popover().unwrap();
        assert!(popover.is_mapped() && popover.width() > 0);
        assert!(
            popover.width() <= width,
            "player menu overflow at {width}: {}",
            popover.width()
        );
        capture(&ui.window, &format!("player-menu-{width}"));
        settings.popdown();
        let stats = find_widget(controls.root.upcast_ref(), &|widget| {
            widget
                .downcast_ref::<gtk::MenuButton>()
                .is_some_and(|button| button.tooltip_text().as_deref() == Some("Playback stats"))
        })
        .unwrap()
        .downcast::<gtk::MenuButton>()
        .unwrap();
        stats.popup();
        let popover = stats.popover().unwrap();
        controls.number("demuxer-cache-duration", 12.5);
        controls.number("speed", 1.25);
        controls.torrent_stats(Some(&madari_native::internal_media::TorrentStats {
            state: "live".into(),
            downloaded: 52428800,
            total: 104857600,
            download_bytes_per_second: 2097152,
            upload_bytes_per_second: 1048576,
            peers: 8,
        }));
        settle();
        controls.tick();
        assert!(controls.stats_visible());
        assert!(
            find_widget(popover.upcast_ref(), &|widget| {
                widget.downcast_ref::<gtk::Label>().is_some_and(|label| {
                    label.text().contains("Connected peers: 8") && label.text().contains("50.0%")
                })
            })
            .is_some()
        );
        assert!(popover.width() <= width, "stats panel overflow at {width}");
        stats.popdown();
    }
    ui.window.destroy();
}
