//! libmpv renders into GTK's framebuffer; no separate player window or IPC socket.
#[path = "mini_player.rs"]
mod mini_player;
use crate::app::{Ui, playback::Target};
use adw::prelude::*;
use gtk::glib;
use libmpv2::{
    Mpv,
    render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType},
};
use madari_model::{Error, ErrorCode, ItemKey, Progress, Result};
use madari_native::companion::Companion;
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

fn error(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Media, message)
}
fn mpv_error(e: libmpv2::Error) -> Error {
    match e {
        libmpv2::Error::Null => error(
            "The video engine could not initialize. Its numeric locale must be C; restart Madari after updating.",
        ),
        _ => error(format!("Player: {e}")),
    }
}
/// Run once at process startup, before creating the runtime or GTK objects.
/// Keep translated UI/text locales, but preserve libmpv's required numeric locale.
pub fn initialize_locale() {
    gtk::disable_setlocale();
    // SAFETY: startup is single-threaded, both strings are static NUL-terminated
    // strings, and no other code has started using the C locale yet.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
        libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
    }
}

struct GlFunctions(libloading::Library);
impl GlFunctions {
    fn new() -> Result<Self> {
        #[cfg(target_os = "windows")]
        const LIBRARY: &str = "libepoxy-0.dll";
        #[cfg(not(target_os = "windows"))]
        const LIBRARY: &str = "libepoxy.so.0";
        // SAFETY: system libepoxy is the same GL dispatch library used by GTK.
        unsafe { libloading::Library::new(LIBRARY) }
            .map(Self)
            .map_err(|_| error("The embedded player requires libepoxy."))
    }
}
fn address(gl: &GlFunctions, name: &str) -> *mut c_void {
    // dlsym returns the address of Epoxy’s function-pointer variable, not the
    // function itself. Dereference that variable before returning a callable address.
    // The library
    // remains loaded for the complete lifetime of the render context.
    unsafe {
        gl.0.get::<*const *mut c_void>(format!("epoxy_{name}\0").as_bytes())
            .map(|symbol| **symbol)
            .unwrap_or(std::ptr::null_mut())
    }
}
struct Renderer {
    context: Option<RenderContext<'static>>,
    _mpv: Arc<Mpv>,
    gl: GlFunctions,
}

// GTK explicitly supports returning an existing context from create-context.
// Keep it alive across GLArea reparenting so libmpv never loses its video output.
fn preserve_gl_context(area: &gtk::GLArea) -> Rc<RefCell<Option<gtk::gdk::GLContext>>> {
    let context = Rc::new(RefCell::new(None));
    let saved = context.clone();
    area.connect_create_context(move |_| saved.borrow().clone());
    let saved = context.clone();
    area.connect_realize(move |area| {
        if saved.borrow().is_none() {
            *saved.borrow_mut() = area.context();
        }
    });
    context
}
impl Renderer {
    fn new(mpv: Arc<Mpv>, dirty: Arc<AtomicBool>) -> Result<Self> {
        let gl = GlFunctions::new()?;
        let mut context = mpv
            .create_render_context(vec![
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams {
                    get_proc_address: address,
                    ctx: GlFunctions::new()?,
                }),
            ])
            .map_err(mpv_error)?;
        context.set_update_callback(move || {
            dirty.store(true, Ordering::Release);
        });
        // SAFETY: this wrapper owns the Arc keeping Mpv at a stable address and
        // drops the render context first. The context never escapes the wrapper.
        let context =
            unsafe { std::mem::transmute::<RenderContext<'_>, RenderContext<'static>>(context) };
        Ok(Self {
            context: Some(context),
            _mpv: mpv,
            gl,
        })
    }
    fn draw(&self, area: &gtk::GLArea) -> Result<()> {
        let get = address(&self.gl, "glGetIntegerv");
        if get.is_null() {
            return Err(error("OpenGL framebuffer access unavailable."));
        }
        let mut framebuffer = 0;
        // SAFETY: glGetIntegerv has this ABI and GTK has made its GL context current.
        unsafe {
            let get: unsafe extern "C" fn(u32, *mut i32) = std::mem::transmute(get);
            get(0x8CA6, &mut framebuffer);
        }
        self.context
            .as_ref()
            .unwrap()
            .render::<GlFunctions>(
                framebuffer,
                area.width() * area.scale_factor(),
                area.height() * area.scale_factor(),
                true,
            )
            .map_err(mpv_error)
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        self.context.take();
    }
}
use crate::app::playback::controls::{Action, Controls};
use crate::app::playback::engine::{self, Command, Engine, Notice};
enum NextPlayback {
    Source(String, Box<madari_model::Stream>),
    Episode(String),
    Retry,
}
enum Update {
    Progress(Box<Progress>),
    Finished(Result<()>),
}
struct Player {
    ui: std::rc::Weak<Ui>,
    source_choices: RefCell<Vec<(String, madari_model::Stream)>>,
    sources_loading: Cell<bool>,
    renderer: RefCell<Option<Renderer>>,
    render_gl: Rc<RefCell<Option<gtk::gdk::GLContext>>>,
    mpv: Arc<Mpv>,
    controls: Rc<Controls>,
    engine: Engine,
    stats_source: Option<(Arc<madari_native::internal_media::InternalMedia>, String)>,
    last_stats: Cell<Instant>,
    trakt: Option<Rc<crate::app::playback::trakt::Session>>,
    notices: RefCell<std::sync::mpsc::Receiver<Notice>>,
    target: RefCell<Option<Target>>,
    main: glib::WeakRef<adw::ApplicationWindow>,
    original: gtk::Widget,
    host: gtk::Overlay,
    browsed: Cell<bool>,
    mini_layout: mini_player::Layout,
    mini_animating: Cell<bool>,
    mini_key_down: Cell<bool>,
    pip: RefCell<Option<gtk::Window>>,
    pip_fullscreen: Cell<bool>,
    mini: RefCell<Option<gtk::Overlay>>,
    mini_keys: RefCell<Option<gtk::EventControllerKey>>,
    updates: mpsc::UnboundedSender<Update>,
    key: ItemKey,
    video: String,
    title: String,
    context: crate::app::playback::PlaybackContext,
    next: RefCell<Option<NextPlayback>>,
    offset: u64,
    position: Cell<Option<u64>>,
    duration: Cell<Option<u64>>,
    loaded: Cell<bool>,
    finished: Cell<bool>,
    moving: Cell<bool>,
    last_save: Cell<Instant>,
    last_ui: Cell<Instant>,
    started: Instant,
    awaiting_next: Cell<bool>,
    chapters: RefCell<Vec<crate::app::playback::engine::Chapter>>,
    next_hint: RefCell<Option<gtk::Box>>,
    next_hint_dismissed: Cell<bool>,
    intro_hint: RefCell<Option<gtk::Button>>,
    intro_target: Cell<Option<u64>>,
}
impl Drop for Player {
    fn drop(&mut self) {
        // Also cover teardown before normal finish (for example a closed host).
        if let Some(context) = self.render_gl.borrow().as_ref() {
            context.make_current();
        }
        self.renderer.borrow_mut().take();
    }
}
impl Player {
    fn progress(&self, completed: bool) -> Option<Progress> {
        let position = self.position.get()?;
        let duration = self.duration.get();
        Some(Progress {
            metadata: self.context.metadata.clone(),
            binge_group: self.context.group.clone(),
            source_provider: Some(self.context.provider.clone()),
            key: self.key.clone(),
            video_id: self.video.clone(),
            position_ms: if completed {
                duration.unwrap_or(position)
            } else {
                duration.map_or(position, |d| position.min(d))
            },
            duration_ms: duration,
            completed,
        })
    }
    fn finish(&self, result: Result<()>, completed: bool) {
        let completed = completed || self.awaiting_next.get();
        if self.finished.replace(true) {
            return;
        }
        if let Some(trakt) = &self.trakt {
            trakt.finish(self.position.get(), self.duration.get(), self.loaded.get());
        }
        if let Some(progress) = self.progress(completed) {
            let _ = self.updates.send(Update::Progress(Box::new(progress)));
        }
        if let Some(context) = self.render_gl.borrow().as_ref() {
            context.make_current();
        }
        self.renderer.borrow_mut().take();
        self.engine.shutdown();
        let _ = self.updates.send(Update::Finished(result));
    }
    fn restore(&self) {
        if let Some(main) = self.main.upgrade()
            && let Some(keys) = self.mini_keys.borrow_mut().take()
        {
            main.remove_controller(&keys);
        }
        self.mini.borrow_mut().take();
        if self.controls.root.parent().as_ref() == Some(self.host.upcast_ref()) {
            self.host.remove_overlay(&self.controls.root);
        }
        self.host.set_child(gtk::Widget::NONE);
        self.original.set_visible(true);
        self.original.set_sensitive(true);

        if let Some(pip) = self.pip.borrow_mut().take() {
            pip.set_child(gtk::Widget::NONE);
            pip.destroy();
        }
        if let Some(main) = self.main.upgrade() {
            main.set_content(Some(&self.original));
            let continuing = self.next.borrow().is_some()
                || self
                    .ui
                    .upgrade()
                    .is_some_and(|ui| ui.after_playback.borrow().is_some());
            if !continuing {
                main.unfullscreen();
            }
        }
    }
    fn schedule_mini(self: &Rc<Self>) {
        if self.mini_animating.get() {
            return;
        }
        if self.moving.replace(true) {
            return;
        }
        self.controls.close_menus();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let Some(p) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if p.finished.get() {
                return glib::ControlFlow::Break;
            }
            if p.controls.menus_open() {
                return glib::ControlFlow::Continue;
            }
            p.toggle_mini();
            p.moving.set(false);
            glib::ControlFlow::Break
        });
    }
    fn toggle_mini(self: &Rc<Self>) {
        let (Some(main), Some(ui)) = (self.main.upgrade(), self.ui.upgrade()) else {
            return;
        };
        if self.pip.borrow().is_some() {
            self.return_from_pip();
            return;
        }
        GtkWindowExt::set_focus(&main, gtk::Widget::NONE);
        // Keep GLArea parented and mapped: only resize/reposition the overlay.
        // Removing it would destroy the GL context and force a video-track reattach.
        let mini = self.mini.borrow_mut().take();
        if mini.is_some() {
            if let Some(task) = ui.request_abort.borrow_mut().take() {
                task.abort();
                ui.view_generation
                    .set(ui.view_generation.get().wrapping_add(1));
                ui.busy.set(false);
                ui.content.set_sensitive(true);
                main.set_cursor_from_name(None);
            }
            self.animate_mini(false);
            self.controls.root.grab_focus();
        } else {
            // In-app layout changes must not change the window's fullscreen state.
            self.original.set_visible(true);
            self.original.set_sensitive(true);
            *self.mini.borrow_mut() = Some(self.host.clone());
            self.animate_mini(true);
            ui.busy.set(false);
            if self.browsed.replace(true) {
                return;
            }
            let meta = ui
                .playback_metadata
                .borrow()
                .get(&(self.key.content_type.clone(), self.key.item_id.clone()))
                .cloned();
            if let Some(meta) = meta {
                let core = ui.core.borrow().as_ref().unwrap().clone();
                let key = self.key.clone();
                let current = self.progress(false);
                ui.run(async move { core.snapshot().await }, move |ui, snapshot| {
                    let saved = snapshot.library.iter().any(|item| item.key == key);
                    let mut progress = snapshot.progress;
                    if let Some(current) = current {
                        progress.push(current);
                    }
                    ui.detail_view(key, meta, saved, progress);
                });
            } else {
                ui.details(self.key.clone(), None);
            }
        }
    }
    fn schedule_pip(self: &Rc<Self>, return_only: bool) {
        if self.moving.replace(true) {
            return;
        }
        self.controls.close_menus();
        let weak = Rc::downgrade(self);
        // Leave the current GTK click/focus dispatch before moving its widgets
        // to another window. Reparenting inside button release is unsafe.
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let Some(p) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if (p.controls.menus_open() || p.mini_animating.get()) && !p.finished.get() {
                return glib::ControlFlow::Continue;
            }
            if !p.finished.get() {
                if return_only {
                    p.return_from_pip();
                } else {
                    p.pip();
                }
            }
            p.moving.set(false);
            glib::ControlFlow::Break
        });
    }

    fn pip(self: &Rc<Self>) {
        if self.pip.borrow().is_some() {
            self.return_from_pip();
            return;
        }
        let Some(main) = self.main.upgrade() else {
            return;
        };
        self.controls.close_menus();
        self.controls.set_compact(true);
        self.pip_fullscreen.set(main.is_fullscreen());
        main.unfullscreen();
        GtkWindowExt::set_focus(&main, gtk::Widget::NONE);
        self.host.remove_overlay(&self.controls.root);
        main.set_content(gtk::Widget::NONE);
        let placeholder = adw::StatusPage::builder()
            .icon_name("video-display-symbolic")
            .title("Playing in picture in picture")
            .description("Your video continues in its own window.")
            .build();
        let restore = gtk::Button::with_label("Return to player");
        restore.add_css_class("suggested-action");
        restore.set_halign(gtk::Align::Center);
        let w = Rc::downgrade(self);
        restore.connect_clicked(move |_| {
            if let Some(p) = w.upgrade() {
                p.schedule_pip(true);
            }
        });
        placeholder.set_child(Some(&restore));
        main.set_content(Some(&placeholder));
        let pip = gtk::Window::builder()
            .title("Madari · Picture in picture")
            .default_width(560)
            .default_height(315)
            .resizable(true)
            .transient_for(&main)
            .build();
        pip.set_application(main.application().as_ref());
        pip.add_css_class("madari-window");
        let w = Rc::downgrade(self);
        pip.connect_close_request(move |_| {
            if let Some(p) = w.upgrade() {
                p.schedule_pip(true);
            }
            glib::Propagation::Stop
        });
        *self.pip.borrow_mut() = Some(pip.clone());
        pip.set_child(Some(&self.controls.root));
        pip.present();
        self.controls.root.grab_focus();
    }
    fn return_from_pip(&self) {
        let pip = self.pip.borrow_mut().take();
        let Some(pip) = pip else {
            return;
        };
        GtkWindowExt::set_focus(&pip, gtk::Widget::NONE);
        pip.set_child(gtk::Widget::NONE);
        self.controls.set_compact(false);
        if let Some(main) = self.main.upgrade() {
            self.host.add_overlay(&self.controls.root);
            main.set_content(Some(&self.host));
            if self.pip_fullscreen.replace(false) {
                main.fullscreen();
            }
            main.present();
        }
        pip.destroy();
        self.controls.root.grab_focus();
    }
    fn window(&self) -> Option<gtk::Window> {
        self.controls.root.root().and_downcast::<gtk::Window>()
    }
    fn fit(&self, value: &str) {
        self.controls.set_fit(value);
        for (name, value) in [
            ("keepaspect", if value == "stretch" { "no" } else { "yes" }),
            (
                "video-unscaled",
                if value == "original" { "yes" } else { "no" },
            ),
            ("panscan", if value == "fill" { "1" } else { "0" }),
            ("video-zoom", "0"),
        ] {
            self.engine.set(name, value);
        }
        self.controls.message(match value {
            "fill" => "Fill / crop",
            "stretch" => "Stretch",
            "original" => "Original size",
            _ => "Fit to window",
        });
    }
    fn episodes(self: &Rc<Self>) {
        self.controls.close_menus();
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let dialog = adw::Dialog::builder()
            .title("Episodes")
            .content_width(760)
            .content_height(620)
            .build();
        let weak = Rc::downgrade(self);
        let weak_dialog = dialog.downgrade();
        let mut progress = self.context.progress.clone();
        if let Some(current) = self.progress(false) {
            progress.push(current);
        }
        let content = ui.episode_browser(
            &self.key,
            &self.context.episodes,
            &progress,
            Some(&self.video),
            Rc::new(move |id| {
                if let Some(p) = weak.upgrade() {
                    if id == p.video {
                        if let Some(d) = weak_dialog.upgrade() {
                            d.close();
                        }
                        return;
                    }
                    *p.next.borrow_mut() = Some(NextPlayback::Episode(id));
                    if let Some(d) = weak_dialog.upgrade() {
                        d.close();
                    }
                    let weak = Rc::downgrade(&p);
                    glib::idle_add_local_once(move || {
                        if let Some(p) = weak.upgrade() {
                            p.finish(Ok(()), false);
                        }
                    });
                }
            }),
        );
        content.set_margin_start(20);
        content.set_margin_end(20);
        content.set_margin_bottom(20);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new("Episodes", &self.title)));
        toolbar.add_top_bar(&header);
        if let Some(season_picker) = content.first_child() {
            content.remove(&season_picker);
            let seasons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            seasons.set_margin_start(20);
            seasons.set_margin_end(20);
            seasons.set_margin_top(8);
            seasons.set_margin_bottom(12);
            seasons.append(&season_picker);
            toolbar.add_top_bar(&seasons);
        }
        toolbar.set_content(Some(&scroll));
        dialog.set_child(Some(&toolbar));
        dialog.present(self.window().as_ref());
    }
    fn update_intro_hint(self: &Rc<Self>) {
        let target = if self.key.content_type == "series"
            && self.controls.seekable.get()
            && !self.awaiting_next.get()
            && self.mini.borrow().is_none()
            && self.pip.borrow().is_none()
        {
            self.position.get().and_then(|position| {
                crate::app::playback::up_next::intro_end(
                    position,
                    &self.chapters.borrow(),
                    self.offset,
                )
            })
        } else {
            None
        };
        self.intro_target.set(target);
        if target.is_none() {
            if let Some(button) = self.intro_hint.borrow_mut().take() {
                self.controls.root.remove_overlay(&button);
            }
            return;
        }
        if self.intro_hint.borrow().is_some() {
            return;
        }
        let button = gtk::Button::with_label("Skip intro");
        button.add_css_class("skip-intro");
        self.controls.place_skip_action(&button);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(player) = weak.upgrade()
                && let Some(target) = player.intro_target.get()
                && player.controls.seekable.get()
            {
                let seconds = target.saturating_sub(player.offset) as f64 / 1000.0;
                player
                    .engine
                    .command(&["seek", &seconds.to_string(), "absolute+exact"]);
            }
        });
        self.controls.root.add_overlay(&button);
        *self.intro_hint.borrow_mut() = Some(button);
    }
    fn hide_next_hint(&self) {
        if let Some(card) = self.next_hint.borrow_mut().take() {
            self.controls.root.remove_overlay(&card);
        }
    }
    fn update_next_hint(self: &Rc<Self>) {
        let eligible =
            self.key.content_type == "series"
                && !self.next_hint_dismissed.get()
                && !self.awaiting_next.get()
                && self.mini.borrow().is_none()
                && self.pip.borrow().is_none()
                && self.position.get().zip(self.duration.get()).is_some_and(
                    |(position, duration)| {
                        crate::app::playback::up_next::suggest(
                            position,
                            duration,
                            &self.chapters.borrow(),
                            self.offset,
                        )
                    },
                );
        if !eligible {
            self.hide_next_hint();
            return;
        }
        if self.next_hint.borrow().is_some() {
            return;
        }
        let Some(next) = madari_core::next_episode(
            &self.context.episodes,
            &self.video,
            &crate::app::browsing::episodes::today(),
        ) else {
            return;
        };
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        card.add_css_class("skip-actions");
        self.controls.place_skip_action(&card);
        let play = gtk::Button::with_label("Next episode");
        play.set_tooltip_text(Some(&crate::app::browsing::episodes::episode_title(next)));
        play.add_css_class("skip-intro");
        let next_id = next.id.clone();
        let weak = Rc::downgrade(self);
        play.connect_clicked(move |_| {
            if let Some(player) = weak.upgrade() {
                *player.next.borrow_mut() = Some(NextPlayback::Episode(next_id.clone()));
                player.finish(Ok(()), true);
            }
        });
        card.append(&play);
        let dismiss = gtk::Button::from_icon_name("window-close-symbolic");
        dismiss.add_css_class("skip-dismiss");
        dismiss.set_tooltip_text(Some("Keep watching"));
        dismiss.update_property(&[gtk::accessible::Property::Label("Keep watching")]);
        let weak = Rc::downgrade(self);
        dismiss.connect_clicked(move |_| {
            if let Some(player) = weak.upgrade() {
                player.next_hint_dismissed.set(true);
                player.hide_next_hint();
            }
        });
        card.append(&dismiss);
        self.controls.root.add_overlay(&card);
        *self.next_hint.borrow_mut() = Some(card);
    }
    fn up_next(self: &Rc<Self>) -> bool {
        self.hide_next_hint();
        if self.key.content_type != "series" {
            return false;
        }
        let Some(next) = madari_core::next_episode(
            &self.context.episodes,
            &self.video,
            &crate::app::browsing::episodes::today(),
        ) else {
            return false;
        };
        if self.awaiting_next.replace(true) {
            return true;
        }
        self.controls.close_menus();
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        card.add_css_class("skip-actions");
        self.controls.place_skip_action(&card);
        let play = gtk::Button::with_label("Next episode · 10s");
        play.add_css_class("skip-intro");
        play.set_tooltip_text(Some(&crate::app::browsing::episodes::episode_title(next)));
        let countdown = play.clone();
        let cancel = gtk::Button::from_icon_name("window-close-symbolic");
        cancel.add_css_class("skip-dismiss");
        cancel.set_tooltip_text(Some("Not now"));
        cancel.update_property(&[gtk::accessible::Property::Label("Not now")]);
        card.append(&play);
        card.append(&cancel);
        self.controls.root.add_overlay(&card);
        self.controls.root.set_cursor_from_name(None);
        play.grab_focus();
        let id = next.id.clone();
        let weak = Rc::downgrade(self);
        let next_id = id.clone();
        play.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                *p.next.borrow_mut() = Some(NextPlayback::Episode(next_id.clone()));
                p.finish(Ok(()), true);
            }
        });
        let weak = Rc::downgrade(self);
        cancel.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.finish(Ok(()), true);
            }
        });
        let weak = Rc::downgrade(self);
        let mut remaining = 10;
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(p) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if p.finished.get() {
                return glib::ControlFlow::Break;
            }
            remaining -= 1;
            if remaining == 0 {
                *p.next.borrow_mut() = Some(NextPlayback::Episode(id.clone()));
                p.finish(Ok(()), true);
                return glib::ControlFlow::Break;
            }
            countdown.set_label(&format!("Next episode · {remaining}s"));
            glib::ControlFlow::Continue
        });
        true
    }
    fn load_sources(self: &Rc<Self>) {
        if self.sources_loading.replace(true) {
            return;
        }
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let Some(core) = ui.core.borrow().as_ref().cloned() else {
            return;
        };
        self.controls.source_status("Loading video sources…", false);
        let request = madari_model::ResourceRequest {
            resource: madari_model::Resource::Stream,
            content_type: self.key.content_type.clone(),
            id: self.video.clone(),
            extra: Default::default(),
        };
        let task = ui.runtime.spawn(async move {
            Ok::<_, madari_model::Error>((core.query_all(request).await?, core.snapshot().await?))
        });
        let w = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = task.await;
            let Some(p) = w.upgrade() else {
                return;
            };
            p.sources_loading.set(false);
            if p.finished.get() {
                return;
            }
            let Ok(Ok((results, snapshot))) = result else {
                p.controls
                    .source_status("Could not load video sources.", true);
                return;
            };
            let mut choices = Vec::new();
            for result in results {
                if let Ok(madari_model::ResourceData::Stream(streams)) = result.result {
                    choices.extend(
                        streams
                            .into_iter()
                            .map(|s| (result.installation_id.clone(), s)),
                    );
                }
            }
            let labels = choices
                .iter()
                .map(|(provider, stream)| {
                    let selected = p.context.current_source
                        == crate::app::playback::fingerprint(provider, stream);
                    let name = snapshot
                        .addons
                        .iter()
                        .find(|a| &a.installation_id == provider)
                        .map_or("Addon", |a| a.manifest.name.as_str());
                    (provider.clone(), name.to_owned(), stream.clone(), selected)
                })
                .collect::<Vec<_>>();
            *p.source_choices.borrow_mut() = choices;
            p.controls.source_choices(&labels);
        });
    }
    fn action(self: &Rc<Self>, action: Action) {
        if self.awaiting_next.get() {
            if matches!(action, Action::Escape | Action::Stop) {
                self.finish(Ok(()), true);
            }
            return;
        }
        if self.finished.get() {
            return;
        }
        match action {
            Action::Pause => self.engine.command(&["cycle", "pause"]),
            Action::Mute => self.engine.command(&["cycle", "mute"]),
            Action::Seek(delta) => {
                if self.controls.seekable.get() {
                    self.engine
                        .command(&["seek", &delta.to_string(), "relative"]);
                    self.controls.message(&format!("{delta:+.0} seconds"));
                } else {
                    self.controls
                        .message("Seeking is unavailable for this stream");
                }
            }
            Action::Volume(delta) => {
                let value = (self.controls.current_volume.get() + delta).clamp(0.0, 100.0);
                self.engine.set("volume", value);
                self.controls.message(&format!("Volume {value:.0}%"));
            }
            Action::Speed(delta) => {
                let value = (self.controls.current_speed.get() + delta).clamp(0.25, 3.0);
                self.engine.set("speed", value);
                self.controls.message(&format!("{value}× playback"));
            }
            Action::Fullscreen => {
                if self.mini.borrow().is_some() {
                    self.schedule_mini();
                    return;
                }
                if let Some(w) = self.window() {
                    if w.is_fullscreen() {
                        w.unfullscreen();
                    } else {
                        w.fullscreen();
                    }
                }
            }
            Action::Escape => {
                if let Some(w) = self.window() {
                    if w.is_fullscreen() {
                        w.unfullscreen();
                    } else if self.pip.borrow().is_some() {
                        self.schedule_pip(true);
                    }
                }
            }
            Action::Mini => self.schedule_mini(),
            Action::Pip => {
                if self.mini.borrow().is_some() {
                    self.schedule_mini();
                    let weak = Rc::downgrade(self);
                    glib::timeout_add_local(Duration::from_millis(50), move || {
                        let Some(p) = weak.upgrade() else {
                            return glib::ControlFlow::Break;
                        };
                        if p.finished.get() {
                            return glib::ControlFlow::Break;
                        }
                        if p.moving.get() || p.mini_animating.get() {
                            return glib::ControlFlow::Continue;
                        }
                        p.schedule_pip(false);
                        glib::ControlFlow::Break
                    });
                } else {
                    self.schedule_pip(false);
                }
            }
            Action::Stop => self.finish(Ok(()), false),
            Action::Sources => self.load_sources(),
            Action::ChooseSource(index) => {
                let choice = self.source_choices.borrow().get(index).cloned();
                if let Some((provider, stream)) = choice {
                    *self.next.borrow_mut() =
                        Some(NextPlayback::Source(provider, Box::new(stream)));
                    self.controls.close_menus();
                    let w = Rc::downgrade(self);
                    glib::timeout_add_local(Duration::from_millis(16), move || {
                        let Some(p) = w.upgrade() else {
                            return glib::ControlFlow::Break;
                        };
                        if p.finished.get() {
                            return glib::ControlFlow::Break;
                        }
                        if p.controls.menus_open() {
                            return glib::ControlFlow::Continue;
                        }
                        p.finish(Ok(()), false);
                        glib::ControlFlow::Break
                    });
                }
            }
            Action::PreviousEpisode | Action::NextEpisode => {
                if self.key.content_type != "series" {
                    return;
                }
                let episode = if action == Action::PreviousEpisode {
                    madari_core::previous_episode(
                        &self.context.episodes,
                        &self.video,
                        &crate::app::browsing::episodes::today(),
                    )
                } else {
                    madari_core::next_episode(
                        &self.context.episodes,
                        &self.video,
                        &crate::app::browsing::episodes::today(),
                    )
                };
                if let Some(episode) = episode {
                    *self.next.borrow_mut() = Some(NextPlayback::Episode(episode.id.clone()));
                    self.finish(Ok(()), false);
                }
            }
            Action::Episodes => self.episodes(),
            Action::Audio => self.engine.command(&["cycle", "aid"]),
            Action::Subtitles => self.engine.command(&["cycle", "sid"]),
            Action::Fit => self.fit(self.controls.next_fit()),
            Action::Set(name, value) => {
                if name == "madari-fit" {
                    self.fit(&value);
                } else {
                    self.engine.set(&name, &value);
                    if name == "speed" {
                        self.controls.message(&format!("{value}× playback"));
                    }
                }
            }
            Action::Run(args) => self
                .engine
                .command(&args.iter().map(String::as_str).collect::<Vec<_>>()),
            Action::Help => {
                self.controls.close_menus();
                let d = adw::AlertDialog::builder()
                    .heading("Player shortcuts")
                    .body(crate::app::playback::controls::SHORTCUTS)
                    .build();
                d.add_response("close", "Close");
                d.present(self.window().as_ref());
            }
            Action::OpenSubtitle => {
                self.controls.close_menus();
                let dialog = gtk::FileDialog::builder()
                    .title("Open subtitles")
                    .modal(true)
                    .build();
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Subtitle files"));
                for pattern in ["*.srt", "*.ass", "*.ssa", "*.vtt", "*.sub"] {
                    filter.add_pattern(pattern);
                }
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));
                let parent = self.window();
                let w = Rc::downgrade(self);
                glib::spawn_future_local(async move {
                    if let Ok(file) = dialog.open_future(parent.as_ref()).await
                        && let Some(p) = w.upgrade()
                        && !p.finished.get()
                    {
                        p.engine.command(&["sub-add", &file.uri(), "select"]);
                    }
                });
            }
        }
    }
    fn poll(self: &Rc<Self>) {
        if self.awaiting_next.get() {
            return;
        }
        for notice in self.notices.borrow().try_iter().take(256) {
            match notice {
                Notice::Loaded => {
                    self.loaded.set(true);
                    self.controls.loaded();
                }
                Notice::Number(name, value) => {
                    if name == "time-pos" && value >= 0.0 {
                        self.position
                            .set(Some((value * 1000.0) as u64 + self.offset));
                    }
                    if name == "duration" && value >= 0.0 {
                        self.duration
                            .set(Some((value * 1000.0) as u64 + self.offset));
                    }
                    self.controls.number(&name, value);
                }
                Notice::Flag(name, value) => self.controls.flag(&name, value),
                Notice::Tracks(tracks) => self.controls.refresh_tracks(&tracks),
                Notice::Chapters(chapters) => {
                    self.controls.refresh_chapters(&chapters);
                    *self.chapters.borrow_mut() = chapters;
                }
                Notice::Ended(eof) => {
                    let completed =
                        eof && self.loaded.get()
                            && self.position.get().zip(self.duration.get()).is_some_and(
                                |(p, d)| d > 0 && p >= d.saturating_sub((d / 20).min(1500)),
                            );
                    if completed && let Some(trakt) = &self.trakt {
                        trakt.finish(self.position.get(), self.duration.get(), self.loaded.get());
                    }
                    if completed && self.up_next() {
                        return;
                    }
                    self.finish(Ok(()), completed);
                    break;
                }
                Notice::Error(message) => {
                    *self.next.borrow_mut() = Some(NextPlayback::Retry);
                    self.finish(Err(error(message)), false);
                    break;
                }
                Notice::Warning(message) => self.controls.message(&message),
            }
        }
        if self.finished.get() {
            return;
        }
        if !self.loaded.get() && self.started.elapsed() > Duration::from_secs(60) {
            *self.next.borrow_mut() = Some(NextPlayback::Retry);
            self.finish(
                Err(error("Playback did not start within one minute.")),
                false,
            );
            return;
        }
        if self.last_ui.get().elapsed() > Duration::from_millis(200) {
            self.last_ui.set(Instant::now());
            if let Some(position) = self.position.get() {
                self.controls.position(
                    position.saturating_sub(self.offset) as f64 / 1000.0,
                    self.offset,
                );
            }
            self.controls.tick();
            if let Some(trakt) = &self.trakt {
                trakt.observe(
                    self.position.get(),
                    self.duration.get(),
                    self.controls.reporting_paused(),
                    self.loaded.get(),
                );
            }
            self.update_intro_hint();
            self.update_next_hint();
        }
        if self.controls.stats_visible()
            && self.last_stats.get().elapsed() >= Duration::from_secs(1)
        {
            self.last_stats.set(Instant::now());
            if let Some((media, token)) = &self.stats_source {
                self.controls.torrent_stats(media.stats(token).as_ref());
            }
        }
        if self.last_save.get().elapsed() > Duration::from_secs(2) {
            self.last_save.set(Instant::now());
            if let Some(progress) = self.progress(false) {
                let _ = self.updates.send(Update::Progress(Box::new(progress)));
            }
        }
    }
}
impl Ui {
    pub(in crate::app) fn embedded_player(
        self: &Rc<Self>,
        key: ItemKey,
        video: String,
        target: Target,
        companion: Option<Companion>,
        token: Option<String>,
    ) {
        let mpv = Mpv::with_initializer(|init| {
            for (name, value) in [
                ("config", "no"),
                ("load-scripts", "no"),
                ("ytdl", "no"),
                ("terminal", "no"),
                ("vo", "libmpv"),
                ("hwdec", "auto-safe"),
                ("idle", "yes"),
                ("access-references", "no"),
                ("cache", "yes"),
                ("demuxer-max-bytes", "67108864"),
                ("demuxer-max-back-bytes", "16777216"),
            ] {
                init.set_option(name, value)?;
            }
            Ok(())
        })
        .map(Arc::new)
        .map_err(mpv_error);
        let mpv = match mpv {
            Ok(m) => m,
            Err(e) => {
                self.toast.add_toast(adw::Toast::new(&e.message));
                if let (Some(c), Some(t)) = (companion, token) {
                    self.runtime.spawn(async move {
                        c.revoke(&t).await;
                    });
                }
                return;
            }
        };
        self.clear();
        let controls = Controls::new();
        let render_gl = preserve_gl_context(&controls.area);
        controls.set_title(&target.title);
        if key.content_type == "series" {
            controls.episode_neighbors(
                madari_core::previous_episode(
                    &target.context.episodes,
                    &video,
                    &crate::app::browsing::episodes::today(),
                ),
                madari_core::next_episode(
                    &target.context.episodes,
                    &video,
                    &crate::app::browsing::episodes::today(),
                ),
            );
            controls.set_episodes_available(!target.context.episodes.is_empty());
        }
        let context = target.context.clone();
        let title = target.title.clone();
        let media = companion
            .as_ref()
            .and_then(Companion::local)
            .map(|local| (local, self.runtime.handle().clone()));
        let (engine, notices) = engine::start_with_media(mpv.clone(), media);
        let (updates, mut receiver) = mpsc::unbounded_channel();
        let original = self.window.content().unwrap();
        self.window.set_content(gtk::Widget::NONE);
        let host = gtk::Overlay::new();
        host.set_child(Some(&original));
        original.set_visible(false);
        host.add_overlay(&controls.root);
        let stats_source = companion
            .as_ref()
            .and_then(Companion::local)
            .zip(token.clone());
        controls.stats_delivery(token.is_some(), stats_source.is_some());
        let trakt = crate::app::playback::trakt::Session::new(
            self,
            &key,
            &video,
            &context.episodes,
            controls.trakt_status.clone(),
        );
        let player = Rc::new(Player {
            trakt,
            ui: Rc::downgrade(self),
            source_choices: RefCell::new(Vec::new()),
            sources_loading: Cell::new(false),
            renderer: RefCell::new(None),
            render_gl,
            mpv,
            controls: controls.clone(),
            engine,
            stats_source,
            last_stats: Cell::new(Instant::now()),
            notices: RefCell::new(notices),
            offset: target.offset_ms,
            target: RefCell::new(Some(target)),
            main: self.window.downgrade(),
            original,
            host: host.clone(),
            browsed: Cell::new(false),
            mini_layout: Default::default(),
            mini_animating: Cell::new(false),
            mini_key_down: Cell::new(false),
            pip: RefCell::new(None),
            pip_fullscreen: Cell::new(false),
            mini: RefCell::new(None),
            mini_keys: RefCell::new(None),
            updates,
            key,
            video,
            title,
            context,
            next: RefCell::new(None),
            position: Cell::new(None),
            duration: Cell::new(None),
            loaded: Cell::new(false),
            finished: Cell::new(false),
            moving: Cell::new(false),
            last_save: Cell::new(Instant::now()),
            last_ui: Cell::new(Instant::now()),
            started: Instant::now(),
            awaiting_next: Cell::new(false),
            chapters: RefCell::new(Vec::new()),
            next_hint: RefCell::new(None),
            next_hint_dismissed: Cell::new(false),
            intro_hint: RefCell::new(None),
            intro_target: Cell::new(None),
        });
        player.mini_handles();
        let w = Rc::downgrade(&player);
        controls.bind(move |action| {
            if let Some(p) = w.upgrade() {
                p.action(action);
            }
        });
        let dirty = Arc::new(AtomicBool::new(true));
        let draw_dirty = dirty.clone();
        let w = Rc::downgrade(&player);
        controls.area.connect_realize(move |area| {
            let Some(p) = w.upgrade() else {
                return;
            };
            if p.finished.get() {
                return;
            }
            area.make_current();
            if let Some(e) = area.error() {
                p.finish(Err(error(format!("OpenGL: {e}"))), false);
                return;
            }
            if p.renderer.borrow().is_some() {
                // A PiP move changes the GTK framebuffer, not the decoder or
                // libmpv render context. The next draw targets the new buffer.
                draw_dirty.store(true, Ordering::Release);
                area.queue_render();
                return;
            }
            match Renderer::new(p.mpv.clone(), draw_dirty.clone()) {
                Ok(renderer) => {
                    *p.renderer.borrow_mut() = Some(renderer);
                    if let Some(target) = p.target.borrow_mut().take() {
                        let _ = p.engine.0.send(Command::Start(Box::new(target)));
                    }
                    draw_dirty.store(true, Ordering::Release);
                }
                Err(e) => p.finish(Err(e), false),
            }
        });
        let w = Rc::downgrade(&player);
        controls.area.connect_render(move |area, _| {
            if let Some(p) = w.upgrade() {
                let result = p.renderer.borrow().as_ref().map(|r| r.draw(area));
                if let Some(Err(e)) = result {
                    p.finish(Err(e), false);
                }
            }
            glib::Propagation::Stop
        });
        let render_dirty = dirty.clone();
        let w = Rc::downgrade(&player);
        controls.area.add_tick_callback(move |area, _| {
            let Some(p) = w.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if p.finished.get() {
                return glib::ControlFlow::Break;
            }
            if render_dirty.swap(false, Ordering::AcqRel) {
                area.queue_render();
            }
            glib::ControlFlow::Continue
        });
        let w = Rc::downgrade(&player);
        glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(p) = w.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if p.finished.get() {
                return glib::ControlFlow::Break;
            }
            p.poll();
            glib::ControlFlow::Continue
        });
        let (stop, stopped) = oneshot::channel();
        *self.player_stop.borrow_mut() = Some(stop);
        let w = Rc::downgrade(&player);
        glib::spawn_future_local(async move {
            if stopped.await.is_ok()
                && let Some(p) = w.upgrade()
            {
                p.finish(Ok(()), false);
            }
        });
        self.busy.set(true);
        self.playback_active.set(true);
        let core = self.core.borrow().as_ref().unwrap().clone();
        let task = self.runtime.spawn(async move {
            let mut result = Ok(());
            while let Some(update) = receiver.recv().await {
                match update {
                    Update::Progress(progress) => {
                        if let Err(e) = core.record_progress(*progress).await {
                            result = Err(e);
                        }
                    }
                    Update::Finished(end) => {
                        if end.is_err() {
                            result = end;
                        }
                        break;
                    }
                }
            }
            if let (Some(c), Some(t)) = (companion, token) {
                c.revoke(&t).await;
            }
            result
        });
        let w = Rc::downgrade(self);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&player);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if let Some(p) = weak.upgrade()
                && p.pip.borrow().is_none()
                && !modifiers.intersects(
                    gtk::gdk::ModifierType::CONTROL_MASK
                        | gtk::gdk::ModifierType::ALT_MASK
                        | gtk::gdk::ModifierType::SUPER_MASK,
                )
                && matches!(key.name().as_deref(), Some("i" | "I"))
                && crate::app::playback::controls::shortcuts_focused(&p.controls.root)
            {
                if !p.mini_key_down.replace(true) {
                    p.schedule_mini();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&player);
        keys.connect_key_released(move |_, key, _, _| {
            if matches!(key.name().as_deref(), Some("i" | "I"))
                && let Some(p) = weak.upgrade()
            {
                p.mini_key_down.set(false);
            }
        });
        self.window.add_controller(keys.clone());
        *player.mini_keys.borrow_mut() = Some(keys);
        // Own the player until progress/ticket cleanup is complete; widget signal
        // callbacks use weak references to avoid retaining discarded video pages.
        glib::spawn_future_local(async move {
            let result = task.await;
            let was_mini = player.mini.borrow().is_some();
            player.restore();
            if let Some(ui) = w.upgrade() {
                ui.player_stop.borrow_mut().take();
                ui.playback_active.set(false);
                if !was_mini {
                    ui.busy.set(false);
                }
                match result {
                    Ok(Err(e)) => ui.toast.add_toast(adw::Toast::new(&e.message)),
                    Err(_) => ui
                        .toast
                        .add_toast(adw::Toast::new("The player stopped unexpectedly.")),
                    _ => (),
                };
                if ui.closing.get() {
                    ui.window.close();
                } else if let Some(after) = ui.after_playback.borrow_mut().take() {
                    after(ui.clone());
                } else {
                    let next = player.next.borrow_mut().take();
                    match next {
                        Some(NextPlayback::Source(provider, stream)) => {
                            ui.start_source(
                                player.key.clone(),
                                player.video.clone(),
                                *stream,
                                provider,
                                None,
                                player.context.transcode,
                            );
                        }
                        Some(NextPlayback::Episode(video)) => ui.continue_playback(
                            player.key.clone(),
                            video,
                            player.context.clone(),
                            true,
                        ),
                        Some(NextPlayback::Retry) => ui.continue_playback(
                            player.key.clone(),
                            player.video.clone(),
                            player.context.clone(),
                            false,
                        ),
                        None if was_mini => (),
                        None => ui.refresh(),
                    }
                }
            }
        });
        self.window.set_content(Some(&host));
        controls.root.grab_focus();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gl_dispatch_resolves_executable_code_not_data() {
        let gl = super::GlFunctions::new().unwrap();
        let mappings = std::fs::read_to_string("/proc/self/maps").unwrap();
        for name in ["glGetString", "glGetIntegerv", "glBindFramebuffer"] {
            let pointer = super::address(&gl, name) as usize;
            assert_ne!(pointer, 0);
            let executable = mappings.lines().any(|line| {
                let mut fields = line.split_whitespace();
                let (start, end) = fields.next().unwrap().split_once('-').unwrap();
                let permissions = fields.next().unwrap();
                let start = usize::from_str_radix(start, 16).unwrap();
                let end = usize::from_str_radix(end, 16).unwrap();
                start <= pointer && pointer < end && permissions.contains('x')
            });
            assert!(
                executable,
                "{name} must resolve to executable memory, not Epoxy's data slot"
            );
        }
    }

    #[test]
    fn startup_locale_allows_libmpv_initialization() {
        // Isolate process-global locale changes from concurrent Rust tests.
        if std::env::var_os("MADARI_LOCALE_TEST_CHILD").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "app::playback::embedded::tests::startup_locale_allows_libmpv_initialization",
                    "--nocapture",
                ])
                .env("MADARI_LOCALE_TEST_CHILD", "1")
                .env("LC_ALL", "en_US.UTF-8")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        super::initialize_locale();
        // SAFETY: querying the locale returns a valid NUL-terminated C string.
        let numeric = unsafe {
            std::ffi::CStr::from_ptr(libc::setlocale(libc::LC_NUMERIC, std::ptr::null()))
        };
        assert_eq!(numeric.to_bytes(), b"C");
        let mpv = libmpv2::Mpv::with_initializer(|init| {
            init.set_option("vo", "libmpv")?;
            init.set_option("terminal", "no")?;
            Ok(())
        });
        assert!(mpv.is_ok(), "libmpv must initialize after locale setup");
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "tests/gl.rs"]
mod gl_tests;

#[cfg(test)]
#[path = "tests/pip.rs"]
mod pip_tests;
