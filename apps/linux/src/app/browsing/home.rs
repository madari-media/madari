use super::*;
use madari_model::{ItemKey, LibraryEntry, Meta, Resource, ResourceData, ResourceRequest};
use std::collections::BTreeMap;

fn key_for(id: &str, meta: &Meta) -> ItemKey {
    ItemKey {
        installation_id: id.into(),
        content_type: meta.content_type.clone(),
        item_id: meta.id.clone(),
    }
}
pub(in crate::app) fn rail() -> (gtk::ScrolledWindow, gtk::Box) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&row)
        .min_content_height(326)
        .build();
    (scroll, row)
}
impl Ui {
    pub(in crate::app) fn remember_metadata(&self, resolved: &madari_core::ResolvedMetadata) {
        self.playback_metadata.borrow_mut().insert(
            (resolved.meta.content_type.clone(), resolved.meta.id.clone()),
            resolved.meta.clone(),
        );
        self.playback_titles.borrow_mut().insert(
            (resolved.meta.content_type.clone(), resolved.meta.id.clone()),
            resolved.meta.name.clone(),
        );
        for name in ["poster", "background", "logo"] {
            if let (Some(url), Some(provider)) = (
                resolved.meta.extra.get(name).and_then(|v| v.as_str()),
                resolved.field_providers.get(name),
            ) {
                self.artwork_policies
                    .borrow_mut()
                    .insert(url.into(), self.local_artwork.borrow().contains(provider));
            }
        }
    }
    pub(in crate::app) fn loading_page(self: &Rc<Self>, detail: bool) {
        self.clear();
        self.page_loading.set(true);
        self.content.append(&self.back_button());
        self.content.append(&brand_logo(64));
        if detail {
            self.content
                .append(&crate::app::interface::loading::hero_skeleton());
        } else {
            self.content
                .append(&crate::app::interface::loading::shimmer(280, 32));
        }
        let (scroll, row) = rail();
        crate::app::interface::loading::poster_row(&row);
        self.content.append(&scroll);
    }
    pub(in crate::app) fn loading_error(self: &Rc<Self>, message: &str) {
        if self.page_loading.replace(false) {
            let retry = self.page_retry.borrow_mut().take();
            self.clear();
            if let Some(retry) = retry {
                self.content
                    .append(&self.button("Retry", move |ui| retry(ui)));
            }
            self.content.append(&self.back_button());
            self.content
                .append(&label("Couldn’t load this page", "section-title"));
            self.content.append(&label(message, "hero-description"));
        } else {
            self.toast.add_toast(adw::Toast::new(message));
        }
    }
    pub(in crate::app) fn picker_add(self: &Rc<Self>, profiles: Vec<Profile>) {
        if profiles.is_empty() {
            self.create_dialog(None);
            return;
        }
        let adults: Vec<_> = profiles.into_iter().filter(|p| !p.kids).collect();
        let fields = form();
        let names: Vec<_> = adults.iter().map(|p| p.name.as_str()).collect();
        let select = gtk::DropDown::from_strings(&names);
        let pin = entry("Profile PIN, if set", true);
        fields.append(&label("Regular profile", "heading"));
        fields.append(&select);
        fields.append(&pin);
        self.dialog(
            "Add a profile",
            "Choose a regular profile to manage this new profile.",
            &fields,
            "Continue",
            move |ui| {
                let Some(adult) = adults.get(select.selected() as usize) else {
                    return;
                };
                let id = adult.id.clone();
                let pin = pin.text().to_string();
                let profiles = ui.profiles.clone();
                ui.run(
                    async move {
                        let session = profiles.unlock(id, pin.clone()).await?;
                        profiles.authorize_settings(session.clone(), pin).await?;
                        Ok(session)
                    },
                    |ui, session| ui.create_dialog(Some(session)),
                );
            },
        );
    }
    pub(in crate::app) fn artwork(
        self: &Rc<Self>,
        picture: &gtk::Picture,
        host: &gtk::Overlay,
        url: Option<&str>,
        provider: &str,
    ) {
        let Some(url) = url.and_then(|u| url::Url::parse(u).ok()) else {
            return;
        };
        host.set_overflow(gtk::Overflow::Hidden);
        let shimmer = crate::app::interface::loading::shimmer(-1, -1);
        shimmer.set_halign(gtk::Align::Fill);
        shimmer.set_valign(gtk::Align::Fill);
        host.add_overlay(&shimmer);
        let host = host.downgrade();
        let shimmer = shimmer.downgrade();
        let allow_local = self
            .artwork_policies
            .borrow()
            .get(url.as_str())
            .copied()
            .unwrap_or_else(|| self.local_artwork.borrow().contains(provider));
        let weak_ui = Rc::downgrade(self);
        let request = RefCell::new(Some((url, host, shimmer)));
        let last = Cell::new(0);
        picture.add_tick_callback(move |picture, clock| {
            if clock.frame_time() - last.get() < 200_000 {
                return glib::ControlFlow::Continue;
            }
            last.set(clock.frame_time());
            if !crate::app::interface::loading::in_view(picture) {
                return glib::ControlFlow::Continue;
            }
            let Some(ui) = weak_ui.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some((url, host, shimmer)) = request.borrow_mut().take() else {
                return glib::ControlFlow::Break;
            };
            let loader = ui.artwork_loader.clone();
            let weak = picture.downgrade();
            let edge = if host
                .upgrade()
                .is_some_and(|h| (1..=255).contains(&h.height_request()))
            {
                360
            } else {
                1600
            };
            let task = ui
                .runtime
                .spawn(async move { loader.load(url, allow_local, edge).await });
            ui.artwork_tasks.borrow_mut().push(task.abort_handle());
            glib::spawn_future_local(async move {
                let result = task.await;
                if let (Some(host), Some(shimmer)) = (host.upgrade(), shimmer.upgrade()) {
                    host.remove_overlay(&shimmer);
                }
                let Some(picture) = weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(Ok(pixels)) => {
                        let bytes = glib::Bytes::from_owned(pixels.data);
                        let texture = gtk::gdk::MemoryTexture::new(
                            pixels.width as i32,
                            pixels.height as i32,
                            gtk::gdk::MemoryFormat::R8g8b8a8,
                            &bytes,
                            pixels.width as usize * 4,
                        );
                        picture.set_paintable(Some(&texture));
                        crate::app::interface::motion::fade_in(&picture);
                    }
                    _ => picture.set_tooltip_text(Some("Artwork unavailable")),
                }
            });
            glib::ControlFlow::Break
        });
    }

    pub(in crate::app) fn poster(self: &Rc<Self>, key: ItemKey, meta: Meta) -> gtk::Button {
        self.poster_progress(key, meta, None)
    }
    pub(in crate::app) fn poster_progress(
        self: &Rc<Self>,
        key: ItemKey,
        meta: Meta,
        progress: Option<f64>,
    ) -> gtk::Button {
        self.poster_action(key, meta, progress, None)
    }
    pub(in crate::app) fn poster_action(
        self: &Rc<Self>,
        key: ItemKey,
        meta: Meta,
        progress: Option<f64>,
        action: Option<Rc<dyn Fn(Rc<Ui>)>>,
    ) -> gtk::Button {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let art = gtk::Overlay::new();
        art.add_css_class("catalog-poster");
        art.set_halign(gtk::Align::Center);
        art.set_valign(gtk::Align::Start);
        art.add_css_class("poster-art");
        art.set_overflow(gtk::Overflow::Hidden);
        let icon = gtk::Image::from_icon_name("video-x-generic-symbolic");
        icon.set_pixel_size(42);
        icon.add_css_class("dim-label");
        art.set_child(Some(&icon));
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        art.add_overlay(&picture);
        self.artwork(
            &picture,
            &art,
            meta.extra.get("poster").and_then(|v| v.as_str()),
            &key.installation_id,
        );
        card.append(&art);
        if let Some(progress) = progress {
            let bar = crate::app::browsing::episodes::viewing_progress(progress, -1);
            bar.set_margin_top(5);
            card.append(&bar);
        }
        let name = label(&meta.name, "poster-name");
        name.set_wrap(false);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(18);
        name.set_width_chars(1);
        card.append(&name);
        let button = self.button(&meta.name.clone(), move |ui| {
            if let Some(action) = &action {
                action(ui);
            } else {
                ui.details(key.clone(), Some(meta.clone()));
            }
        });
        button.add_css_class("poster-card");
        button.set_child(Some(&card));
        button.set_valign(gtk::Align::Start);
        button.set_halign(gtk::Align::Center);
        button
    }
    pub(in crate::app) fn hero(
        self: &Rc<Self>,
        key: ItemKey,
        meta: &Meta,
        detail: bool,
        progress: Option<&[madari_model::Progress]>,
    ) -> gtk::Overlay {
        self.playback_titles
            .borrow_mut()
            .entry((meta.content_type.clone(), meta.id.clone()))
            .or_insert_with(|| meta.name.clone());
        let hero = gtk::Overlay::new();
        hero.add_css_class("hero");
        hero.set_overflow(gtk::Overflow::Hidden);
        if detail {
            hero.add_css_class("detail-hero");
        }
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        hero.set_child(Some(&picture));
        let image = meta
            .extra
            .get("background")
            .or_else(|| meta.extra.get("poster"))
            .and_then(|v| v.as_str());
        self.artwork(&picture, &hero, image, &key.installation_id);
        let shade = gtk::Box::new(gtk::Orientation::Vertical, 0);
        shade.add_css_class("hero-shade");
        shade.set_hexpand(true);
        shade.set_vexpand(true);
        hero.add_overlay(&shade);
        let copy = form();
        copy.add_css_class("hero-copy");
        copy.set_valign(gtk::Align::End);
        copy.set_halign(gtk::Align::Fill);
        copy.append(&label(
            if meta.content_type == "series" {
                "SERIES"
            } else if detail {
                "MOVIE"
            } else {
                "FEATURED FILM"
            },
            "eyebrow",
        ));
        let title = label(&meta.name, "hero-title");
        title.set_max_width_chars(26);
        copy.append(&title);
        if detail && let Some(url) = meta.extra.get("logo").and_then(|v| v.as_str()) {
            let host = gtk::Overlay::new();
            host.set_size_request(-1, 80);
            host.set_halign(gtk::Align::Start);
            let picture = gtk::Picture::new();
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.set_can_shrink(true);
            picture.update_property(&[gtk::accessible::Property::Label(&meta.name)]);
            host.set_child(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
            host.add_overlay(&picture);
            let fallback = title.downgrade();
            picture.connect_paintable_notify(move |p| {
                if let Some(title) = fallback.upgrade() {
                    title.set_visible(p.paintable().is_none());
                }
            });
            let weak_host = host.downgrade();
            picture.connect_notify_local(Some("tooltip-text"), move |p, _| {
                if p.tooltip_text().as_deref() == Some("Artwork unavailable")
                    && let Some(host) = weak_host.upgrade()
                {
                    host.set_visible(false);
                }
            });
            self.artwork(&picture, &host, Some(url), &key.installation_id);
            copy.append(&host);
        }
        let facts = crate::app::browsing::details::hero_facts(meta);
        if !facts.is_empty() {
            copy.append(&label(&facts.join("  ·  "), "hero-meta"));
        }
        if let Some(description) = meta
            .extra
            .get("description")
            .or_else(|| meta.extra.get("overview"))
            .and_then(|v| v.as_str())
        {
            let text = label(description, "hero-description");
            text.set_max_width_chars(64);
            text.set_lines(3);
            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            copy.append(&text);
        }
        let actions = responsive::row(12);
        let play_key = key.clone();
        let play_meta = meta.clone();
        let recommendation = progress.and_then(|p| {
            madari_core::continue_episode(
                &meta.videos,
                p,
                &key,
                &crate::app::browsing::episodes::today(),
            )
        });
        let play_label = recommendation
            .map(|v| {
                let resume = progress.is_some_and(|items| {
                    items.iter().any(|p| {
                        p.key == key && p.video_id == v.id && !p.completed && p.position_ms > 0
                    })
                });
                format!(
                    "{} S{} · E{}",
                    if resume { "Resume" } else { "Play" },
                    v.season.unwrap_or(1),
                    v.episode.unwrap_or(1)
                )
            })
            .unwrap_or_else(|| {
                if !meta.videos.is_empty()
                    && progress.is_some_and(|p| p.iter().any(|p| p.key == key && p.completed))
                {
                    "Watch again".into()
                } else {
                    "▶  Play".into()
                }
            });
        let play = self.button(&play_label, move |ui| {
            ui.resume_title(play_key.clone(), play_meta.clone());
        });
        play.add_css_class("play-button");
        actions.append(&play);
        if !detail {
            let meta = meta.clone();
            let more = self.button("More info", move |ui| {
                ui.details(key.clone(), Some(meta.clone()))
            });
            more.add_css_class("secondary-button");
            actions.append(&more);
        }
        copy.append(&actions);
        hero.add_overlay(&copy);
        hero.set_measure_overlay(&copy, true);
        hero
    }
    fn show_featured(
        self: &Rc<Self>,
        container: &gtk::Box,
        pool: Rc<RefCell<Vec<(ItemKey, Meta)>>>,
        selected: Option<usize>,
    ) {
        let count = pool.borrow().len();
        if count == 0 {
            return;
        }
        let index = selected
            .unwrap_or_else(|| (uuid::Uuid::new_v4().as_u128() % count as u128) as usize)
            % count;
        let (key, meta) = pool.borrow()[index].clone();
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        let hero = self.hero(key, &meta, false, None);
        crate::app::interface::motion::fade_in(&hero);
        container.append(&hero);
        let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        navigation.set_halign(gtk::Align::End);
        for (title, forward) in [("Previous", false), ("Next", true)] {
            let weak = container.downgrade();
            let pool = pool.clone();
            let button = self.button(title, move |ui| {
                if let Some(container) = weak.upgrade() {
                    let count = pool.borrow().len();
                    if count == 0 {
                        return;
                    }
                    let next = if forward {
                        (index + 1) % count
                    } else {
                        (index + count - 1) % count
                    };
                    ui.show_featured(&container, pool.clone(), Some(next));
                }
            });
            button.set_tooltip_text(Some(if forward {
                "Next featured title"
            } else {
                "Previous featured title"
            }));
            navigation.append(&button);
        }
        container.append(&navigation);
    }

    pub(in crate::app) fn dashboard(self: &Rc<Self>, snapshot: PublicSnapshot) {
        self.dashboard_filter(snapshot, None);
    }
    pub(in crate::app) fn dashboard_filter(
        self: &Rc<Self>,
        snapshot: PublicSnapshot,
        filter: Option<&'static str>,
    ) {
        self.clear();
        self.build_navigation();
        self.select_section(match filter {
            Some("movie") => "Movies",
            Some("series") => "Series",
            _ => "Home",
        });
        let hero_box = form();
        hero_box.append(&crate::app::interface::loading::hero_skeleton());
        self.content.append(&hero_box);
        self.saved_rails(&snapshot, false);
        let hero_set = Rc::new(Cell::new(false));
        let featured = Rc::new(RefCell::new(Vec::<(ItemKey, Meta)>::new()));
        let generation = self.view_generation.get();
        let mut catalogs = 0;
        let pending = Rc::new(Cell::new(0usize));
        for addon in snapshot.addons.iter().filter(|a| a.enabled) {
            for catalog in addon
                .manifest
                .catalogs
                .iter()
                .filter(|c| filter.is_none_or(|f| c.content_type == f))
                .filter(|c| !c.search_only() && !c.extra.iter().any(|e| e.name == "lastVideosIds"))
            {
                catalogs += 1;
                let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let title = label(
                    catalog.name.as_deref().unwrap_or(&catalog.id),
                    "section-title",
                );
                title.set_hexpand(true);
                heading.append(&title);
                let id = addon.installation_id.clone();
                let cat = catalog.clone();
                heading.append(&self.button("Explore →", move |ui| {
                    ui.catalog_options(id.clone(), cat.clone())
                }));
                self.content.append(&heading);
                let (scroll, row) = rail();
                crate::app::interface::loading::poster_row(&row);
                self.content.append(&scroll);
                if catalog
                    .extra
                    .iter()
                    .any(|e| e.is_required && e.name != "skip")
                {
                    while let Some(child) = row.first_child() {
                        row.remove(&child);
                    }
                    row.append(&label(
                        "Choose a filter to explore this collection.",
                        "dim-label",
                    ));
                    continue;
                }
                let request = ResourceRequest {
                    resource: Resource::Catalog,
                    content_type: catalog.content_type.clone(),
                    id: catalog.id.clone(),
                    extra: BTreeMap::new(),
                };
                let core = self.core.borrow().as_ref().unwrap().clone();
                let id = addon.installation_id.clone();
                let provider = id.clone();
                let retry_catalog = catalog.clone();
                let task = self
                    .runtime
                    .spawn(async move { core.query(&provider, request).await });
                let weak = Rc::downgrade(self);
                let weak_row = row.downgrade();
                let hero_box = hero_box.downgrade();
                let hero_set = hero_set.clone();
                let featured = featured.clone();
                pending.set(pending.get() + 1);
                let pending = pending.clone();
                glib::spawn_future_local(async move {
                    let result = task.await;
                    let (Some(ui), Some(row)) = (weak.upgrade(), weak_row.upgrade()) else {
                        return;
                    };
                    if ui.view_generation.get() != generation {
                        return;
                    }
                    while let Some(child) = row.first_child() {
                        row.remove(&child);
                    }
                    let failed = !matches!(&result, Ok(Ok(ResourceData::Catalog(_))));
                    match result {
                        Ok(Ok(ResourceData::Catalog(items))) => {
                            if items.is_empty() {
                                row.append(&label(
                                    "No titles in this collection yet.",
                                    "dim-label",
                                ));
                            }
                            {
                                let mut pool = featured.borrow_mut();
                                for meta in items.iter().take(24) {
                                    if !pool.iter().any(|(_, saved)| {
                                        saved.id == meta.id
                                            && saved.content_type == meta.content_type
                                    }) {
                                        pool.push((key_for(&id, meta), meta.clone()));
                                    }
                                }
                            }
                            if !featured.borrow().is_empty()
                                && !hero_set.replace(true)
                                && let Some(hero_box) = hero_box.upgrade()
                            {
                                ui.show_featured(&hero_box, featured.clone(), None);
                            }
                            for meta in items.into_iter().take(24) {
                                let key = key_for(&id, &meta);
                                let card = ui.poster(key, meta);
                                crate::app::interface::motion::fade_in(&card);
                                row.append(&card);
                            }
                        }
                        Ok(Err(e)) => row.append(&label(&e.message, "dim-label")),
                        _ => row.append(&label("This collection could not load.", "dim-label")),
                    }
                    if failed {
                        row.append(&ui.button("Retry collection", move |ui| {
                            ui.catalog_options(id.clone(), retry_catalog.clone());
                        }));
                    }
                    pending.set(pending.get().saturating_sub(1));
                    if pending.get() == 0
                        && !hero_set.get()
                        && let Some(hero_box) = hero_box.upgrade()
                    {
                        while let Some(child) = hero_box.first_child() {
                            hero_box.remove(&child);
                        }
                    }
                });
            }
        }
        if pending.get() == 0 {
            while let Some(child) = hero_box.first_child() {
                hero_box.remove(&child);
            }
        }
        if catalogs == 0 {
            let empty = form();
            empty.add_css_class("empty-state");
            empty.append(&label("Your next story starts here", "hero-title"));
            empty.append(&label(
                if self
                    .session
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| s.profile.kids)
                {
                    "An adult can choose addons to fill your home with movies and shows."
                } else {
                    "Add your favorite addons in Settings to discover movies and shows."
                },
                "hero-description",
            ));
            self.content.append(&empty);
        }
    }
    pub(in crate::app) fn saved_page(self: &Rc<Self>, snapshot: PublicSnapshot) {
        self.clear();
        self.section_heading("My list", None);
        self.saved_rails(&snapshot, true);
        self.trakt_library(snapshot);
    }
    fn saved_rails(self: &Rc<Self>, snapshot: &PublicSnapshot, include_library: bool) {
        let continuing: Vec<_> = snapshot
            .progress
            .iter()
            .rev()
            .filter(|p| {
                p.position_ms > 0
                    && (!p.completed || p.key.content_type == "series")
                    && !snapshot.hidden_continue.contains(&p.key)
            })
            .map(|p| p.key.clone())
            .collect();
        let mut sections = vec![("Continue watching", continuing)];
        if include_library {
            sections.push((
                "Saved for later",
                snapshot.library.iter().map(|i| i.key.clone()).collect(),
            ));
        }
        for (title, keys) in sections {
            let mut seen = std::collections::HashSet::new();
            let keys: Vec<_> = keys
                .into_iter()
                .filter(|k| {
                    seen.insert((
                        k.installation_id.clone(),
                        k.content_type.clone(),
                        k.item_id.clone(),
                    ))
                })
                .take(30)
                .collect();
            if keys.is_empty() {
                continue;
            }
            let section = form();
            if title == "Continue watching" {
                section.add_css_class("continue-section");
            }
            section.append(&label(title, "section-title"));
            self.content.append(&section);
            let (scroll, row) = rail();
            let continuing = title == "Continue watching";
            if continuing {
                scroll.set_min_content_height(190);
            }
            section.append(&scroll);
            let mut holders = Vec::new();
            for key in &keys {
                let holder = form();
                if continuing {
                    holder.append(&crate::app::interface::loading::shimmer(300, 190));
                } else {
                    holder.append(&crate::app::interface::loading::poster_skeleton());
                }
                row.append(&holder);
                holders.push((key.clone(), holder.downgrade()));
            }
            let core = self.core.borrow().as_ref().unwrap().clone();
            let task = self
                .runtime
                .spawn(async move { core.continue_metadata(&keys).await });
            self.artwork_tasks.borrow_mut().push(task.abort_handle());
            let weak = Rc::downgrade(self);
            let generation = self.view_generation.get();
            let progress = snapshot.progress.clone();
            let library = snapshot.library.clone();
            let continuing = title == "Continue watching";
            glib::spawn_future_local(async move {
                let result = task.await;
                let Some(ui) = weak.upgrade() else {
                    return;
                };
                if ui.view_generation.get() != generation {
                    return;
                }
                let resolved = result.ok().and_then(|r| r.ok()).unwrap_or_default();
                for (key, holder) in holders {
                    let Some(holder) = holder.upgrade() else {
                        continue;
                    };
                    while let Some(child) = holder.first_child() {
                        holder.remove(&child);
                    }
                    if let Some((_, meta)) = resolved.iter().find(|(k, _)| *k == key) {
                        ui.remember_metadata(meta);
                        if continuing {
                            ui.continue_card(&holder, key, meta.meta.clone(), &progress);
                        } else {
                            holder.append(&ui.poster(key, meta.meta.clone()));
                        }
                    } else {
                        let preview = ui
                            .playback_metadata
                            .borrow()
                            .get(&(key.content_type.clone(), key.item_id.clone()))
                            .cloned();
                        let fallback = preview.unwrap_or_else(|| Meta {
                            id: key.item_id.clone(),
                            content_type: key.content_type.clone(),
                            name: library
                                .iter()
                                .find(|item| item.key == key)
                                .map(|item| item.title.clone())
                                .or_else(|| {
                                    ui.playback_titles
                                        .borrow()
                                        .get(&(key.content_type.clone(), key.item_id.clone()))
                                        .cloned()
                                })
                                .unwrap_or_else(|| key.item_id.clone()),
                            videos: Vec::new(),
                            extra: Default::default(),
                        });
                        holder.set_tooltip_text(Some("Details could not be refreshed. Your saved title and playback history are still available."));
                        if continuing {
                            ui.continue_card(&holder, key, fallback, &progress);
                        } else {
                            holder.append(&ui.poster(key, fallback));
                        }
                    }
                }
            });
        }
        if include_library && snapshot.library.is_empty() {
            self.content.append(&label(
                "Save a movie or show from its detail page to find it here.",
                "empty-state",
            ));
        }
    }
    pub(in crate::app) fn detail_view(
        self: &Rc<Self>,
        key: ItemKey,
        meta: Meta,
        saved: bool,
        progress: Vec<madari_model::Progress>,
    ) {
        self.clear();
        self.content.append(&self.back_button());
        self.content
            .append(&self.hero(key.clone(), &meta, true, Some(&progress)));
        let save_key = key.clone();
        let title = meta.name.clone();
        let preview = meta.clone();
        let save = self.button(
            if saved {
                "✓  In my list"
            } else {
                "+  My list"
            },
            move |ui| {
                let core = ui.core.borrow().as_ref().unwrap().clone();
                let key = save_key.clone();
                let title = title.clone();
                let lookup = key.clone();
                let preview = preview.clone();
                let metadata = Some(preview.preview());
                ui.run(
                    async move {
                        if saved {
                            core.remove_item(&key).await
                        } else {
                            core.save_item(LibraryEntry {
                                key,
                                title,
                                metadata,
                            })
                            .await
                        }
                    },
                    move |ui, _| ui.details(lookup, Some(preview)),
                );
            },
        );
        save.set_halign(gtk::Align::Start);
        save.add_css_class("secondary-button");
        let actions = responsive::row(12);
        actions.append(&save);
        self.detail_links(&actions, &meta);
        self.content.append(&actions);
        self.rich_details(&key, &meta, &progress);
    }
}
