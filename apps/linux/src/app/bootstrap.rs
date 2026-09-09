//! Window construction, application startup, and shutdown.
use super::*;

pub(in crate::app) fn data_directory() -> PathBuf {
    std::env::var_os("MADARI_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            #[cfg(target_os = "windows")]
            {
                PathBuf::from(
                    std::env::var_os("LOCALAPPDATA")
                        .expect("LOCALAPPDATA or MADARI_DATA_DIR must be set"),
                )
                .join("Madari")
            }
            #[cfg(not(target_os = "windows"))]
            {
                std::env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(
                            std::env::var_os("HOME").expect("HOME or MADARI_DATA_DIR must be set"),
                        )
                        .join(".local/share")
                    })
                    .join("madari")
            }
        })
}
pub(in crate::app) fn build_ui(
    app: &adw::Application,
    runtime: Arc<Runtime>,
    profiles: Profiles,
    cache_dir: PathBuf,
) -> Rc<Ui> {
    interface::styles::install_style();
    let content = form();
    content.set_hexpand(true);
    content.set_halign(gtk::Align::Fill);
    content.set_margin_top(32);
    content.set_margin_bottom(32);
    content.set_margin_start(32);
    content.set_margin_end(32);
    let clamp = adw::Clamp::builder()
        .maximum_size(1440)
        .tightening_threshold(1440)
        .hexpand(true)
        .child(&content)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .vexpand(true)
        .build();
    let toast = adw::ToastOverlay::new();
    let page_surface = gtk::Overlay::new();
    page_surface.set_child(Some(&scroll));
    toast.set_child(Some(&page_surface));
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let chrome = adw::HeaderBar::new();
    chrome.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    chrome.set_decoration_layout(Some(":close"));
    chrome.set_show_start_title_buttons(false);
    chrome.add_css_class("minimal-chrome");
    layout.append(&chrome);
    let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    navigation.add_css_class("navbar");
    navigation.set_margin_start(32);
    navigation.set_margin_end(32);
    navigation.set_visible(false);
    let navigation_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&navigation)
        .propagate_natural_height(true)
        .build();
    let navigation_clamp = adw::Clamp::builder()
        .maximum_size(1440)
        .tightening_threshold(1440)
        .child(&navigation_scroll)
        .build();
    layout.append(&navigation_clamp);
    layout.append(&toast);
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Madari")
        .default_width(1100)
        .default_height(740)
        .content(&layout)
        .build();
    window.add_css_class("madari-window");
    responsive::install(&window, &content, &navigation);
    let internal_companion =
        madari_native::companion::Companion::internal(data_directory().join("media"));
    let media = internal_companion.local().unwrap();
    let trakt_queue = playback::trakt::Queue::new(&runtime, profiles.clone());
    let shutdown_trakt = trakt_queue.clone();
    let shutdown_runtime = runtime.clone();
    app.connect_shutdown(move |_| {
        shutdown_runtime.block_on(async {
            tokio::join!(media.shutdown(), shutdown_trakt.flush());
        });
    });
    Rc::new(Ui {
        window: window.clone(),
        content,
        navigation,
        page_surface,
        toast,
        runtime: runtime.clone(),
        profiles: profiles.clone(),
        trakt_queue,
        session: RefCell::new(None),
        core: RefCell::new(None),
        editing: Cell::new(false),
        busy: Cell::new(false),
        button_loading: RefCell::new(None),
        companion: RefCell::new(
            match (
                std::env::var("MADARI_COMPANION_URL"),
                std::env::var("MADARI_COMPANION_API_KEY"),
            ) {
                (Ok(url), Ok(key)) => madari_native::companion::Companion::new(&url, key)
                    .ok()
                    .or_else(|| Some(internal_companion.clone())),
                _ => Some(internal_companion.clone()),
            },
        ),
        internal_companion,
        after_playback: RefCell::new(None),
        player_stop: RefCell::new(None),
        request_abort: RefCell::new(None),
        closing: Cell::new(false),
        playback_active: Cell::new(false),
        view_generation: Cell::new(0),
        page_loading: Cell::new(false),
        page_retry: RefCell::new(None),
        preferences: RefCell::new(None),
        preferences_pages: RefCell::new(Vec::new()),
        artwork_loader: Arc::new(artwork::ArtworkCache::new(cache_dir)),
        artwork_tasks: RefCell::new(Vec::new()),
        playback_metadata: RefCell::new(Default::default()),
        playback_titles: RefCell::new(Default::default()),
        local_artwork: RefCell::new(Default::default()),
        artwork_policies: RefCell::new(Default::default()),
    })
}

pub(crate) fn run() -> glib::ExitCode {
    embedded::initialize_locale();
    let runtime = Arc::new(Runtime::new().expect("create async runtime"));
    let dir = data_directory();
    madari_native::private_directory(&dir).expect("create private application directory");
    let profiles = match runtime.block_on(Profiles::open(dir.join("profiles.sqlite"))) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Cannot open profiles: {}", e.message);
            return glib::ExitCode::FAILURE;
        }
    };
    let app = adw::Application::builder()
        .application_id("io.github.madari.Madari")
        .build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let ui = build_ui(
            app,
            runtime.clone(),
            profiles.clone(),
            data_directory().join("cache/artwork"),
        );
        if std::fs::read_dir(data_directory().join("media/torrents"))
            .is_ok_and(|mut files| files.next().is_some())
        {
            let media = ui.internal_companion.local().unwrap();
            let task = ui.runtime.spawn(async move { media.start().await });
            let weak = Rc::downgrade(&ui);
            glib::spawn_future_local(async move {
                if let Ok(Err(error)) = task.await
                    && let Some(ui) = weak.upgrade()
                {
                    ui.toast.add_toast(adw::Toast::new(&error.message));
                }
            });
        }
        let window = ui.window.clone();
        // The window owns the controller until it is destroyed; callbacks use weak refs.
        let controller = ui.clone();
        window.connect_destroy(move |_| {
            let _ = &controller;
        });
        let weak = Rc::downgrade(&ui);
        window.connect_close_request(move |_| {
            if let Some(ui) = weak.upgrade() {
                if ui.playback_active.get() {
                    ui.closing.set(true);
                    if let Some(stop) = ui.player_stop.borrow_mut().take() {
                        let _ = stop.send(());
                    }
                    return glib::Propagation::Stop;
                }
                if let Some(task) = ui.request_abort.borrow_mut().take() {
                    task.abort();
                }
            }
            glib::Propagation::Proceed
        });
        ui.picker();
        window.present();
    });
    app.run()
}
