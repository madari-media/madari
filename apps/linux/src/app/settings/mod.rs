pub(in crate::app) mod playback;
mod trakt;

use super::*;

impl Ui {
    pub(in crate::app) fn preferences_view(self: &Rc<Self>, snapshot: PublicSnapshot) {
        let window = if let Some(window) = self.preferences.borrow().as_ref() {
            window.clone()
        } else {
            let window = adw::PreferencesWindow::builder()
                .title("Settings")
                .transient_for(&self.window)
                .modal(true)
                .default_width(self.window.width().clamp(360, 760))
                .default_height(self.window.height().clamp(280, 720))
                .search_enabled(true)
                .build();
            let weak = Rc::downgrade(self);
            window.connect_close_request(move |_| {
                let Some(ui) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if ui.busy.get() {
                    return glib::Propagation::Stop;
                }
                ui.preferences.borrow_mut().take();
                ui.preferences_pages.borrow_mut().clear();
                ui.editing.set(false);
                let profiles = ui.profiles.clone();
                let session = ui.session.borrow().as_ref().unwrap().clone();
                ui.run(
                    async move { profiles.lock_settings(session).await },
                    |ui, ()| ui.refresh(),
                );
                glib::Propagation::Proceed
            });
            *self.preferences.borrow_mut() = Some(window.clone());
            window
        };
        let visible = window.visible_page_name();
        for page in self.preferences_pages.borrow_mut().drain(..) {
            window.remove(&page);
        }
        let profile = self.session.borrow().as_ref().unwrap().profile.clone();
        let personal = adw::PreferencesPage::builder()
            .title("Profile")
            .name("profile")
            .icon_name("avatar-default-symbolic")
            .build();
        let identity = adw::PreferencesGroup::builder()
            .title(&profile.name)
            .description(if profile.kids {
                "Kids profile · protected by the guardian’s PIN"
            } else {
                "Manage this profile and its protection"
            })
            .build();
        let name = adw::EntryRow::builder()
            .title("Profile name")
            .text(&profile.name)
            .build();
        identity.add(&name);
        let (image_field, avatar) = self.avatar_field(&profile.name, profile.avatar.as_deref());
        let image_row = adw::ActionRow::builder().title("Profile image").build();
        image_row.add_suffix(&image_field);
        identity.add(&image_row);
        let pin = adw::PasswordEntryRow::builder().title("New PIN").build();
        if !profile.kids {
            identity.add(&pin);
        }
        let save_row = adw::ActionRow::builder()
            .use_markup(false)
            .title("Save profile changes")
            .subtitle(if profile.kids {
                "Kids profiles use the guardian’s PIN"
            } else {
                "Leave New PIN empty to keep the current PIN. Use 4–8 digits."
            })
            .build();
        let save = self.button("Apply", move |ui| {
            let profiles = ui.profiles.clone();
            let session = ui.session.borrow().as_ref().unwrap().clone();
            let name = name.text().to_string();
            let pin = pin.text().to_string();
            let avatar = avatar.borrow().clone();
            ui.run(
                async move {
                    profiles
                        .update_with_avatar(session, name, pin, avatar)
                        .await
                },
                |ui, p| {
                    ui.session.borrow_mut().as_mut().unwrap().profile = p;
                    ui.refresh();
                },
            );
        });
        save.set_valign(gtk::Align::Center);
        save.add_css_class("suggested-action");
        save_row.add_suffix(&save);
        identity.add(&save_row);
        personal.add(&identity);
        personal.add(&self.trakt_preferences());
        if !profile.kids {
            let family = adw::PreferencesGroup::builder()
                .title("Profiles on this device")
                .build();
            let row = adw::ActionRow::builder()
                .use_markup(false)
                .title("Add profile")
                .subtitle("Create a regular profile or a kids profile")
                .build();
            let button = self.button("Add", |ui| ui.create_profile());
            button.set_valign(gtk::Align::Center);
            row.add_suffix(&button);
            family.add(&row);
            personal.add(&family);
        }
        let addons = adw::PreferencesPage::builder()
            .title("Addons")
            .name("addons")
            .icon_name("application-x-addon-symbolic")
            .build();
        let installed = adw::PreferencesGroup::builder()
            .title("Installed addons")
            .description("Enabled status and order apply only to this profile.")
            .build();
        let install = adw::ActionRow::builder()
            .use_markup(false)
            .title("Install addon")
            .subtitle("Add a configured manifest URL")
            .build();
        let button = self.button("Install", |ui| ui.install(None));
        button.set_valign(gtk::Align::Center);
        install.add_suffix(&button);
        installed.add(&install);
        let ids: Vec<_> = snapshot
            .addons
            .iter()
            .map(|a| a.installation_id.clone())
            .collect();
        for (index, addon) in snapshot.addons.iter().enumerate() {
            let catalogs = &addon.manifest.catalogs;
            let capability = if !catalogs.is_empty() && catalogs.iter().all(|c| c.search_only()) {
                "Search only"
            } else if catalogs.iter().any(|c| c.searchable()) {
                "Browse and search"
            } else if !catalogs.is_empty() {
                "Browse"
            } else {
                "Metadata / playback resources"
            };
            let row = adw::ExpanderRow::new();
            row.set_use_markup(false);
            row.set_title(&addon.manifest.name);
            row.set_subtitle(&format!("{} · {capability}", addon.manifest.version));
            let enabled = adw::SwitchRow::builder()
                .title("Enabled for this profile")
                .active(addon.enabled)
                .build();
            let weak = Rc::downgrade(self);
            let id = addon.installation_id.clone();
            enabled.connect_active_notify(move |switch| {
                let Some(ui) = weak.upgrade() else {
                    return;
                };
                if ui.busy.get() {
                    return;
                }
                let core = ui.core.borrow().as_ref().unwrap().clone();
                let id = id.clone();
                let enabled = switch.is_active();
                ui.run(
                    async move { core.set_enabled(&id, enabled).await },
                    |ui, _| ui.refresh(),
                );
            });
            row.add_row(&enabled);
            let config = adw::ActionRow::builder()
                .use_markup(false)
                .title("Configuration")
                .subtitle("Updates affect every profile sharing this installation")
                .build();
            let id = addon.installation_id.clone();
            let button = self.button("Configure", move |ui| ui.install(Some(id.clone())));
            button.set_valign(gtk::Align::Center);
            config.add_suffix(&button);
            row.add_row(&config);
            if index > 0 {
                let order = adw::ActionRow::builder()
                    .use_markup(false)
                    .title("Catalog order")
                    .build();
                let mut reordered = ids.clone();
                reordered.swap(index, index - 1);
                let button = self.button("Move up", move |ui| {
                    let core = ui.core.borrow().as_ref().unwrap().clone();
                    let ids = reordered.clone();
                    ui.run(async move { core.reorder(&ids).await }, |ui, _| {
                        ui.refresh()
                    });
                });
                button.set_valign(gtk::Align::Center);
                order.add_suffix(&button);
                row.add_row(&order);
            }
            let remove = adw::ActionRow::builder()
                .use_markup(false)
                .title("Remove from this profile")
                .subtitle("Other profiles keep their shared installation")
                .build();
            let id = addon.installation_id.clone();
            let title = addon.manifest.name.clone();
            let button = self.button("Remove", move |ui| {
                let id = id.clone();
                ui.dialog(
                    "Remove addon?",
                    &format!("Remove {title} from this profile?"),
                    &form(),
                    "Remove",
                    move |ui| {
                        let core = ui.core.borrow().as_ref().unwrap().clone();
                        let id = id.clone();
                        ui.run(async move { core.remove_addon(&id).await }, |ui, _| {
                            ui.refresh()
                        });
                    },
                );
            });
            button.set_valign(gtk::Align::Center);
            button.add_css_class("destructive-action");
            remove.add_suffix(&button);
            row.add_row(&remove);
            installed.add(&row);
        }
        addons.add(&installed);
        let sharing = adw::PreferencesPage::builder()
            .title("Sharing")
            .name("sharing")
            .icon_name("system-users-symbolic")
            .build();
        let links=adw::PreferencesGroup::builder().title("Share addons").description("Linked profiles share configuration. Their library and viewing progress stay separate.").build();
        for addon in &snapshot.addons {
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            row.set_title(&addon.manifest.name);
            let id = addon.installation_id.clone();
            let button = self.button("Share…", move |ui| ui.share(id.clone()));
            button.set_valign(gtk::Align::Center);
            row.add_suffix(&button);
            links.add(&row);
        }
        sharing.add(&links);
        let companion = adw::PreferencesPage::builder()
            .title("Media server")
            .name("companion")
            .icon_name("network-server-symbolic")
            .build();
        let server = adw::PreferencesGroup::builder()
            .title("Media server")
            .description("Built-in torrent playback uses no listening ports. External servers also support FFmpeg transcoding.")
            .build();
        let row = adw::ActionRow::builder()
            .use_markup(false)
            .title("Connection")
            .subtitle(
                self.companion
                    .borrow()
                    .as_ref()
                    .map_or("Not connected", |c| c.origin()),
            )
            .build();
        row.set_use_markup(false);
        let status = row.downgrade();
        let button = self.button("Configure…", move |ui| {
            ui.companion_settings(status.clone())
        });
        button.set_valign(gtk::Align::Center);
        row.add_suffix(&button);
        server.add(&row);
        companion.add(&server);
        let playback = self.playback_preferences_page(snapshot.playback_preferences);
        for page in [personal, playback, addons, sharing, companion] {
            window.add(&page);
            self.preferences_pages.borrow_mut().push(page);
        }
        if let Some(visible) = visible {
            window.set_visible_page_name(&visible);
        }
        window.present();
    }
}

pub(in crate::app) mod access;
pub(in crate::app) mod addons;
