use super::*;
use madari_native::trakt::{Data, Title};

impl Ui {
    pub(in crate::app) fn trakt_library(self: &Rc<Self>, snapshot: PublicSnapshot) {
        let holder = form();
        self.content.append(&holder);
        let profiles = self.profiles.clone();
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        let task = self
            .runtime
            .spawn(async move { profiles.trakt_data(session).await });
        let weak = Rc::downgrade(self);
        let holder = holder.downgrade();
        let generation = self.view_generation.get();
        glib::spawn_future_local(async move {
            let result = task.await;
            let (Some(ui), Some(holder)) = (weak.upgrade(), holder.upgrade()) else {
                return;
            };
            if ui.view_generation.get() != generation {
                return;
            }
            if let Ok(Ok(Some(data))) = result {
                ui.trakt_collection(&holder, data, snapshot);
            }
        });
    }
    pub(in crate::app) fn trakt_collection(
        self: &Rc<Self>,
        holder: &gtk::Box,
        data: Data,
        snapshot: PublicSnapshot,
    ) {
        holder.append(&label(
            &format!("Trakt · {}", data.username),
            "section-title",
        ));
        let mut lists = vec![
            ("Watchlist".to_owned(), data.watchlist),
            ("Watched history".to_owned(), data.history),
        ];
        lists.extend(data.lists.into_iter().map(|l| (l.name, l.items)));
        let titles: Vec<_> = lists
            .iter()
            .map(|(name, items)| format!("{name} ({})", items.len()))
            .collect();
        let names: Vec<_> = titles.iter().map(String::as_str).collect();
        let select = gtk::DropDown::from_strings(&names);
        select.set_enable_search(true);
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let text = gtk::Label::builder()
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(24)
                .xalign(0.0)
                .build();
            item.set_child(Some(&text));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let (Some(value), Some(text)) = (
                item.item().and_downcast::<gtk::StringObject>(),
                item.child().and_downcast::<gtk::Label>(),
            ) {
                text.set_text(&value.string());
                text.set_tooltip_text(Some(&value.string()));
            }
        });
        select.set_factory(Some(&factory));
        select.set_hexpand(true);
        let chooser = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        chooser.append(&select);
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Refresh Trakt lists and artwork"));
        chooser.append(&refresh);
        holder.append(&chooser);
        let weak = Rc::downgrade(self);
        let host = holder.downgrade();
        let refreshed_snapshot = snapshot.clone();
        refresh.connect_clicked(move |button| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(session) = ui.session.borrow().clone() else {
                return;
            };
            button.set_sensitive(false);
            let spinner = gtk::Spinner::new();
            spinner.start();
            button.set_child(Some(&spinner));
            let profiles = ui.profiles.clone();
            let task = ui
                .runtime
                .spawn(async move { profiles.sync_trakt(session, false).await });
            let weak = weak.clone();
            let host = host.clone();
            let button = button.downgrade();
            let snapshot = refreshed_snapshot.clone();
            let generation = ui.view_generation.get();
            glib::spawn_future_local(async move {
                let result = task.await;
                if let Some(button) = button.upgrade() {
                    button.set_icon_name("view-refresh-symbolic");
                    button.set_sensitive(true);
                }
                let (Some(ui), Some(host)) = (weak.upgrade(), host.upgrade()) else {
                    return;
                };
                if ui.view_generation.get() != generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(data))) => {
                        while let Some(child) = host.first_child() {
                            host.remove(&child);
                        }
                        ui.trakt_collection(&host, data, snapshot);
                    }
                    Ok(Err(e)) => ui.toast.add_toast(adw::Toast::new(&e.message)),
                    _ => ui
                        .toast
                        .add_toast(adw::Toast::new("Could not refresh Trakt.")),
                }
            });
        });
        let content = form();
        holder.append(&content);
        let lists = Rc::new(lists);
        let snapshot = Rc::new(snapshot);
        self.trakt_items(&content, &lists[0].1, snapshot.clone());
        let weak = Rc::downgrade(self);
        select.connect_selected_notify(move |select| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if let Some((_, items)) = lists.get(select.selected() as usize) {
                ui.trakt_items(&content, items, snapshot.clone());
            }
        });
    }
    fn trakt_items(
        self: &Rc<Self>,
        content: &gtk::Box,
        items: &[Title],
        snapshot: Rc<PublicSnapshot>,
    ) {
        while let Some(child) = content.first_child() {
            content.remove(&child);
        }
        if items.is_empty() {
            content.append(&label("Nothing in this Trakt list yet.", "empty-state"));
            return;
        }
        let (scroll, row) = home::rail();
        content.append(&scroll);
        let more = gtk::Button::with_label("Show more");
        more.set_valign(gtk::Align::Center);
        row.append(&more);
        let items = Rc::new(items.to_vec());
        let loaded = Rc::new(Cell::new(0));
        self.trakt_cards(&row, &more, &items, &loaded, &snapshot);
        let weak = Rc::downgrade(self);
        let row_weak = row.downgrade();
        more.connect_clicked(move |button| {
            let (Some(ui), Some(row)) = (weak.upgrade(), row_weak.upgrade()) else {
                return;
            };
            ui.trakt_cards(&row, button, &items, &loaded, &snapshot);
        });
    }
    fn trakt_details(self: &Rc<Self>, title: Title) {
        let Some(core) = self.core.borrow().as_ref().cloned() else {
            return;
        };
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        let profiles = self.profiles.clone();
        self.loading_page(true);
        let retry = title.clone();
        *self.page_retry.borrow_mut() = Some(Rc::new(move |ui| ui.trakt_details(retry.clone())));
        self.run(
            async move {
                let (key, metadata) = title.resolve(&core).await?;
                profiles
                    .apply_trakt_history(session, key.clone(), metadata.meta.clone())
                    .await?;
                let snapshot = core.snapshot().await?;
                let saved = snapshot.library.iter().any(|item| item.key == key);
                Ok((key, metadata, saved, snapshot.progress))
            },
            |ui, (key, metadata, saved, progress)| {
                ui.remember_metadata(&metadata);
                ui.detail_view(key, metadata.meta, saved, progress);
            },
        );
    }
    fn trakt_artwork(self: &Rc<Self>, holder: &gtk::Box, title: Title) {
        // Full Trakt artwork can render immediately. Only missing artwork needs
        // a background addon lookup; clicking always resolves full addon data.
        if title
            .meta()
            .extra
            .get("poster")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty())
        {
            return;
        }
        let Some(core) = self.core.borrow().as_ref().cloned() else {
            return;
        };
        let task = self
            .runtime
            .spawn(async move { title.resolve(&core).await });
        self.artwork_tasks.borrow_mut().push(task.abort_handle());
        let weak = Rc::downgrade(self);
        let holder = holder.downgrade();
        let generation = self.view_generation.get();
        glib::spawn_future_local(async move {
            let result = task.await;
            let (Some(ui), Some(holder)) = (weak.upgrade(), holder.upgrade()) else {
                return;
            };
            if ui.view_generation.get() != generation || holder.parent().is_none() {
                return;
            }
            let Some(button) = holder.first_child().and_downcast::<gtk::Button>() else {
                return;
            };
            match result {
                Ok(Ok((key, metadata))) => {
                    ui.remember_metadata(&metadata);
                    let updated = ui.poster(key, metadata.meta);
                    let child = updated.child();
                    updated.set_child(gtk::Widget::NONE);
                    // Keep the original button and its external-title click
                    // handler alive, including during a press/release gesture.
                    button.set_child(child.as_ref());
                }
                Ok(Err(error)) => button.set_tooltip_text(Some(&error.message)),
                Err(_) => {}
            }
        });
    }
    fn trakt_cards(
        self: &Rc<Self>,
        row: &gtk::Box,
        more: &gtk::Button,
        items: &[Title],
        loaded: &Cell<usize>,
        snapshot: &PublicSnapshot,
    ) {
        let end = (loaded.get() + 30).min(items.len());
        for item in &items[loaded.get()..end] {
            let preview = item.meta();
            let key = item.key(snapshot).unwrap_or_else(|| madari_model::ItemKey {
                installation_id: "trakt".into(),
                content_type: preview.content_type.clone(),
                item_id: preview.id.clone(),
            });
            let preview = item.preview_for(&key);
            let selected = item.clone();
            let action = Rc::new(move |ui: Rc<Ui>| ui.trakt_details(selected.clone()));
            let card = self.poster_action(key, preview, None, Some(action));
            let holder = form();
            holder.append(&card);
            self.trakt_artwork(&holder, item.clone());
            if let (Some(season), Some(episode)) = (item.season, item.episode) {
                holder.append(&label(&format!("S{season} · E{episode}"), "dim-label"));
            }
            row.insert_child_after(&holder, more.prev_sibling().as_ref());
        }
        loaded.set(end);
        more.set_visible(end < items.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use madari_core::{Http, Snapshot, Storage};
    use serde_json::json;
    struct Fixture;
    #[async_trait]
    impl Http for Fixture {
        async fn get_json(&self, _: url::Url, _: bool) -> madari_model::Result<serde_json::Value> {
            Ok(
                json!({"meta":{"id":"tt123","type":"series","name":"Resolved addon title","videos":[{"id":"custom-episode","season":1,"episode":1,"title":"First episode"}]}}),
            )
        }
    }
    #[async_trait]
    impl Storage for Fixture {
        async fn load(&self) -> madari_model::Result<Snapshot> {
            Ok(Snapshot{addons:vec![serde_json::from_value(json!({"installation_id":"metadata","manifest_url":"https://fixture.example/manifest.json","enabled":true,"allow_local":false,"manifest":{"id":"metadata","name":"Metadata","version":"1","resources":["meta"],"types":["series"],"idPrefixes":["tt"],"catalogs":[]}})).unwrap()],..Default::default()})
        }
        async fn compare_and_swap(&self, _: u64, _: Snapshot) -> madari_model::Result<()> {
            Ok(())
        }
    }
    fn find(root: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
        if root.has_css_class(class) {
            return Some(root.clone());
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some(found) = find(&widget, class) {
                return Some(found);
            }
        }
        None
    }
    fn until(mut ready: impl FnMut() -> bool) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            if ready() {
                return;
            }
            assert!(
                std::time::Instant::now() < end,
                "Trakt card did not reach expected state"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "Requires a GTK display; run under Xvfb"]
    fn trakt_card_keeps_click_handler_after_artwork_and_opens_addon_details() {
        adw::init().unwrap();
        let app = adw::Application::builder()
            .application_id("media.madari.TraktClickTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gtk::gio::Cancellable::NONE).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let runtime = Arc::new(Runtime::new().unwrap());
        let profiles = runtime
            .block_on(Profiles::open(dir.path().join("profiles.sqlite")))
            .unwrap();
        let profile = runtime
            .block_on(profiles.create(None, "Fixture".into(), false, String::new()))
            .unwrap();
        let session = runtime
            .block_on(profiles.unlock(profile.id, String::new()))
            .unwrap();
        let ui = crate::app::build_ui(&app, runtime.clone(), profiles, dir.path().join("artwork"));
        let core = Arc::new(Core::new(Arc::new(Fixture), Arc::new(Fixture)));
        let snapshot = runtime.block_on(core.snapshot()).unwrap();
        *ui.core.borrow_mut() = Some(core);
        *ui.session.borrow_mut() = Some(session);
        let holder = form();
        ui.content.append(&holder);
        ui.window.present();
        let title = Title {
            trakt_id: 5,
            imdb: Some("tt123".into()),
            tmdb: None,
            name: "Trakt preview".into(),
            content_type: "series".into(),
            year: None,
            season: None,
            episode: None,
            watched_at: None,
            details: Default::default(),
        };
        ui.trakt_collection(
            &holder,
            Data {
                watchlist: vec![title],
                ..Default::default()
            },
            snapshot,
        );
        let button = find(holder.upcast_ref(), "poster-card")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        until(|| {
            ui.playback_metadata
                .borrow()
                .contains_key(&("series".into(), "tt123".into()))
        });
        assert!(
            button.parent().is_some(),
            "Artwork replaced the clickable card"
        );
        button.emit_clicked();
        until(|| !ui.busy.get() && find(ui.content.upcast_ref(), "detail-hero").is_some());
        assert_eq!(
            ui.playback_metadata.borrow()[&("series".into(), "tt123".into())].videos[0].id,
            "custom-episode"
        );
        ui.window.destroy();
    }
}
