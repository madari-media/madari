//! Shared application state and user actions. Feature modules extend `Ui`.
mod bootstrap;
mod browsing;
mod interface;
mod playback;
mod profiles;
mod settings;
mod transfers;

use adw::prelude::*;
#[cfg(test)]
use bootstrap::build_ui;
use bootstrap::data_directory;
pub(crate) use bootstrap::run;
use gtk::glib;
use interface::widgets::{brand_logo, entry, form, label};
use interface::{artwork, motion, responsive};
use madari_core::{Core, PublicSnapshot};
use madari_model::Result;
use madari_native::profiles::{Profile, ProfileSession, Profiles};
use playback::embedded;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};
use tokio::runtime::Runtime;

type AfterPlayback = Box<dyn FnOnce(Rc<Ui>)>;
type PageRetry = Rc<dyn Fn(Rc<Ui>)>;

struct Ui {
    window: adw::ApplicationWindow,
    content: gtk::Box,
    navigation: gtk::Box,
    page_surface: gtk::Overlay,
    toast: adw::ToastOverlay,
    runtime: Arc<Runtime>,
    profiles: Profiles,
    trakt_queue: playback::trakt::Queue,
    session: RefCell<Option<ProfileSession>>,
    core: RefCell<Option<Arc<Core>>>,
    editing: Cell<bool>,
    busy: Cell<bool>,
    button_loading: RefCell<Option<Box<dyn FnOnce()>>>,
    companion: RefCell<Option<madari_native::companion::Companion>>,
    internal_companion: madari_native::companion::Companion,
    after_playback: RefCell<Option<AfterPlayback>>,
    player_stop: RefCell<Option<tokio::sync::oneshot::Sender<()>>>,
    request_abort: RefCell<Option<tokio::task::AbortHandle>>,
    closing: Cell<bool>,
    playback_active: Cell<bool>,
    view_generation: Cell<u64>,
    page_loading: Cell<bool>,
    page_retry: RefCell<Option<PageRetry>>,
    preferences: RefCell<Option<adw::PreferencesWindow>>,
    preferences_pages: RefCell<Vec<adw::PreferencesPage>>,
    artwork_loader: Arc<artwork::ArtworkCache>,
    artwork_tasks: RefCell<Vec<tokio::task::AbortHandle>>,
    playback_metadata: RefCell<std::collections::HashMap<(String, String), madari_model::Meta>>,
    playback_titles: RefCell<std::collections::HashMap<(String, String), String>>,
    local_artwork: RefCell<std::collections::HashSet<String>>,
    artwork_policies: RefCell<std::collections::HashMap<String, bool>>,
}

impl Ui {
    fn run<T: Send + 'static>(
        self: &Rc<Self>,
        future: impl Future<Output = Result<T>> + Send + 'static,
        done: impl FnOnce(Rc<Self>, T) + 'static,
    ) {
        if self.busy.replace(true) {
            return;
        }
        self.content
            .set_sensitive(self.page_loading.get() || self.button_loading.borrow().is_some());
        if let Some(p) = self.preferences.borrow().as_ref() {
            p.set_sensitive(false);
        }
        self.window.set_cursor_from_name(Some("wait"));
        let task = self.runtime.spawn(future);
        *self.request_abort.borrow_mut() = Some(task.abort_handle());
        let weak = Rc::downgrade(self);
        let generation = self.view_generation.get();
        glib::spawn_future_local(async move {
            let result = task.await;
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if ui.view_generation.get() != generation {
                return;
            }
            ui.request_abort.borrow_mut().take();
            ui.busy.set(false);
            ui.content.set_sensitive(true);
            if let Some(p) = ui.preferences.borrow().as_ref() {
                p.set_sensitive(true);
            }
            ui.window.set_cursor_from_name(None);
            match result {
                Ok(Ok(value)) => {
                    ui.page_loading.set(false);
                    done(ui.clone(), value)
                }
                Ok(Err(error)) => ui.loading_error(&error.message),
                Err(_) => ui.loading_error("The operation could not finish. Please try again."),
            }
            // A callback may start the next step (metadata, sources, preparation).
            // Keep the clicked action loading until that whole chain finishes.
            if !ui.busy.get() {
                ui.finish_button_loading();
            }
        });
    }
    fn finish_button_loading(&self) {
        let reset = self.button_loading.borrow_mut().take();
        if let Some(reset) = reset {
            reset();
        }
    }
    fn clear(&self) {
        self.content.set_valign(gtk::Align::Fill);
        self.content
            .set_margin_top(if self.window.has_css_class("narrow") {
                16
            } else {
                24
            });
        self.finish_button_loading();
        motion::page_change(&self.page_surface);
        self.select_section("");
        self.page_retry.borrow_mut().take();
        for task in self.artwork_tasks.borrow_mut().drain(..) {
            task.abort();
        }
        self.view_generation
            .set(self.view_generation.get().wrapping_add(1));
        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
    }
    fn refresh(self: &Rc<Self>) {
        let core = self.core.borrow().as_ref().unwrap().clone();
        self.run(async move { core.snapshot().await }, |ui, snapshot| {
            ui.home(snapshot)
        });
    }
    fn home(self: &Rc<Self>, snapshot: PublicSnapshot) {
        self.content.set_margin_top(24);
        *self.local_artwork.borrow_mut() = snapshot
            .addons
            .iter()
            .filter(|a| a.allow_local)
            .map(|a| a.installation_id.clone())
            .collect();
        if self.editing.get() {
            self.preferences_view(snapshot);
        } else {
            self.dashboard(snapshot);
        }
    }
}
