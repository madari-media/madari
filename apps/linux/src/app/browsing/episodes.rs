use super::*;
use madari_model::{ItemKey, Meta, Progress, Video};
pub fn today() -> String {
    glib::DateTime::now_utc()
        .and_then(|d| d.format("%Y-%m-%d"))
        .map(|d| d.to_string())
        .unwrap_or_else(|_| "1970-01-01".into())
}
pub fn episode_title(v: &Video) -> String {
    let number = match (v.season, v.episode) {
        (Some(s), Some(e)) => format!("S{s} · E{e}"),
        _ => "Episode".into(),
    };
    format!("{number} · {}", v.title.as_deref().unwrap_or(&v.id))
}
fn progress_text(p: Option<&Progress>) -> String {
    match p {
        Some(p) if p.completed => "Watched".into(),
        Some(p) if p.position_ms > 0 && madari_core::resume_position(p) == 0 => {
            "Play from beginning".into()
        }
        Some(p) if p.position_ms > 0 => format!(
            "Resume at {}:{:02}",
            p.position_ms / 60000,
            p.position_ms / 1000 % 60
        ),
        _ => "Not watched".into(),
    }
}
pub(in crate::app) fn viewing_progress(fraction: f64, width: i32) -> gtk::ProgressBar {
    let bar = gtk::ProgressBar::new();
    bar.add_css_class("viewing-progress");
    bar.set_show_text(false);
    bar.set_can_target(false);
    bar.set_halign(gtk::Align::Start);
    bar.set_size_request(width, -1);
    bar.set_fraction(fraction.clamp(0.0, 1.0));
    bar
}
impl Ui {
    pub(in crate::app) fn resume_saved(self: &Rc<Self>, key: ItemKey, video: String) {
        let Some(core) = self.core.borrow().as_ref().cloned() else {
            return;
        };
        self.run(async move { core.snapshot().await }, move |ui, snapshot| {
            let saved = snapshot
                .progress
                .iter()
                .rev()
                .find(|p| p.key == key && p.video_id == video)
                .or_else(|| snapshot.progress.iter().rev().find(|p| p.key == key));
            let context = crate::app::playback::PlaybackContext {
                group: saved.and_then(|p| p.binge_group.clone()),
                provider: saved
                    .and_then(|p| p.source_provider.clone())
                    .unwrap_or_default(),
                ..Default::default()
            };
            ui.continue_playback(key, video, context, false);
        });
    }
    pub(in crate::app) fn resume_title(self: &Rc<Self>, key: ItemKey, meta: Meta) {
        if meta.content_type != "series"
            && let Some(id) = meta
                .extra
                .get("behaviorHints")
                .and_then(|h| h.get("defaultVideoId"))
                .and_then(|id| id.as_str())
                .filter(|id| !id.is_empty())
        {
            self.resume_saved(key, id.into());
            return;
        }
        if meta.videos.is_empty() && meta.content_type != "series" {
            self.resume_saved(key, meta.id);
            return;
        }
        let core = self.core.borrow().as_ref().unwrap().clone();
        let lookup = key.clone();
        if self.button_loading.borrow().is_none() {
            self.loading_page(true);
        }
        let retry_key = key.clone();
        let retry_meta = meta.clone();
        *self.page_retry.borrow_mut() = Some(Rc::new(move |ui| {
            ui.resume_title(retry_key.clone(), retry_meta.clone())
        }));
        self.run(
            async move {
                let meta = if meta.videos.is_empty() {
                    core.resolve_metadata(&lookup, Some(meta)).await?.meta
                } else {
                    meta
                };
                Ok((meta, core.snapshot().await?))
            },
            move |ui, (meta, snapshot)| {
                let video =
                    madari_core::continue_episode(&meta.videos, &snapshot.progress, &key, &today());
                if let Some(video) = video {
                    ui.playback_metadata.borrow_mut().insert(
                        (key.content_type.clone(), key.item_id.clone()),
                        meta.clone(),
                    );
                    ui.playback_titles.borrow_mut().insert(
                        (key.content_type.clone(), key.item_id.clone()),
                        meta.name.clone(),
                    );
                    ui.resume_saved(key, video.id.clone());
                } else {
                    ui.details(key, Some(meta));
                }
            },
        );
    }
    pub(in crate::app) fn continue_card(
        self: &Rc<Self>,
        holder: &gtk::Box,
        key: ItemKey,
        meta: Meta,
        progress: &[Progress],
    ) {
        let is_series = key.content_type == "series";
        let episode = if !is_series || meta.videos.is_empty() {
            None
        } else {
            let Some(video) = madari_core::continue_episode(&meta.videos, progress, &key, &today())
            else {
                holder.set_visible(false);
                return;
            };
            Some(video)
        };
        let video = episode.map_or_else(
            || {
                progress
                    .iter()
                    .rev()
                    .find(|p| p.key == key)
                    .map_or_else(|| meta.id.clone(), |p| p.video_id.clone())
            },
            |v| v.id.clone(),
        );
        let saved = progress
            .iter()
            .rev()
            .find(|p| p.key == key && p.video_id == video);
        let caption = episode.map(episode_title).unwrap_or_else(|| {
            if is_series {
                "Continue series"
            } else {
                "Continue movie"
            }
            .into()
        });
        holder.set_halign(gtk::Align::Start);
        holder.set_spacing(8);
        let fraction = saved.and_then(|p| {
            p.duration_ms
                .filter(|d| *d > 0)
                .map(|d| p.position_ms as f64 / d as f64)
        });
        self.playback_metadata.borrow_mut().insert(
            (key.content_type.clone(), key.item_id.clone()),
            meta.clone(),
        );
        self.playback_titles.borrow_mut().insert(
            (key.content_type.clone(), key.item_id.clone()),
            meta.name.clone(),
        );
        let surface = gtk::Overlay::new();
        surface.set_size_request(300, 190);
        surface.set_halign(gtk::Align::Start);
        surface.set_valign(gtk::Align::Start);
        surface.set_overflow(gtk::Overflow::Hidden);
        surface.add_css_class("continue-card");
        let artwork = gtk::Overlay::new();
        artwork.set_size_request(300, 190);
        artwork.set_overflow(gtk::Overflow::Hidden);
        artwork.add_css_class("poster-art");
        artwork.set_child(Some(&gtk::Image::from_icon_name(
            "video-x-generic-symbolic",
        )));
        let picture = gtk::Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        artwork.add_overlay(&picture);
        let image = episode
            .and_then(|v| v.extra.get("thumbnail"))
            .and_then(|v| v.as_str())
            .or_else(|| meta.extra.get("background").and_then(|v| v.as_str()))
            .or_else(|| meta.extra.get("poster").and_then(|v| v.as_str()));
        self.artwork(&picture, &artwork, image, &key.installation_id);
        let shade = gtk::Box::new(gtk::Orientation::Vertical, 0);
        shade.add_css_class("continue-shade");
        shade.set_can_target(false);
        artwork.add_overlay(&shade);
        let kind = gtk::Label::new(Some(match meta.content_type.as_str() {
            "movie" => "Movie",
            "series" => "Series",
            other => other,
        }));
        kind.add_css_class("continue-kind");
        kind.set_halign(gtk::Align::Start);
        kind.set_valign(gtk::Align::Start);
        kind.set_margin_top(10);
        kind.set_margin_start(12);
        kind.set_can_target(false);
        artwork.add_overlay(&kind);
        let copy = gtk::Box::new(gtk::Orientation::Vertical, 5);
        copy.set_valign(gtk::Align::End);
        copy.set_can_target(false);
        copy.add_css_class("continue-copy");
        let title = label(&meta.name, "continue-title");
        title.set_lines(2);
        title.set_max_width_chars(28);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        copy.append(&title);
        if episode.is_some() {
            let text = label(&caption, "continue-episode");
            text.set_lines(1);
            text.set_max_width_chars(30);
            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            copy.append(&text);
        }
        let status = if saved.is_some_and(|p| !p.completed && p.position_ms > 0) {
            progress_text(saved)
        } else if is_series && (episode.is_some() || saved.is_some_and(|p| p.completed)) {
            "Play next episode".into()
        } else if is_series {
            "Continue series".into()
        } else {
            "Play movie".into()
        };
        let resume = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let play_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let status_label = label(&status, "continue-status");
        resume.append(&play_icon);
        resume.append(&spinner);
        resume.append(&status_label);
        copy.append(&resume);
        if let Some(fraction) = fraction {
            let bar = viewing_progress(fraction, 268);
            bar.set_margin_top(5);
            copy.append(&bar);
        }
        artwork.add_overlay(&copy);
        let play_key = key.clone();
        let play = gtk::Button::new();
        let action_label = format!("{status} · {} · {caption}", meta.name);
        play.update_property(&[gtk::accessible::Property::Label(&action_label)]);
        play.set_tooltip_text(Some("Click to play · Right-click for details"));
        let details_key = key.clone();
        let details_meta = meta.clone();
        let secondary_click = gtk::GestureClick::new();
        secondary_click.set_button(3);
        secondary_click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        secondary_click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(ui) = weak.upgrade().filter(|ui| !ui.busy.get()) {
                ui.details(details_key.clone(), Some(details_meta.clone()));
            }
        });
        play.add_controller(secondary_click);
        let weak = Rc::downgrade(self);
        let needs_episodes = is_series && meta.videos.is_empty();
        play.connect_clicked(move |button| {
            let Some(ui) = weak.upgrade().filter(|ui| !ui.busy.get()) else {
                return;
            };
            spinner.set_visible(true);
            spinner.start();
            play_icon.set_visible(false);
            status_label.set_text("Finding streams…");
            button.set_sensitive(false);
            button.update_property(&[gtk::accessible::Property::Label("Finding streams")]);
            let button = button.downgrade();
            let spinner = spinner.clone();
            let play_icon = play_icon.clone();
            let status_label = status_label.clone();
            let status = status.clone();
            let action_label = action_label.clone();
            *ui.button_loading.borrow_mut() = Some(Box::new(move || {
                spinner.stop();
                spinner.set_visible(false);
                play_icon.set_visible(true);
                status_label.set_text(&status);
                if let Some(button) = button.upgrade() {
                    button.set_sensitive(true);
                    button.update_property(&[gtk::accessible::Property::Label(&action_label)]);
                }
            }));
            if needs_episodes {
                ui.resume_title(play_key.clone(), meta.clone());
            } else {
                ui.resume_saved(play_key.clone(), video.clone());
            }
            if !ui.busy.get() {
                ui.finish_button_loading();
            }
        });
        play.remove_css_class("compact-action");
        play.add_css_class("continue-play");
        play.set_halign(gtk::Align::Fill);
        play.set_valign(gtk::Align::Fill);
        play.set_child(Some(&artwork));
        surface.set_child(Some(&play));
        let remove_key = key.clone();
        let weak_holder = holder.downgrade();
        let remove = gtk::Button::with_label("Remove");
        let weak = Rc::downgrade(self);
        remove.connect_clicked(move |button| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(core) = ui.core.borrow().as_ref().cloned() else {
                return;
            };
            button.set_sensitive(false);
            let button = button.downgrade();
            let key = remove_key.clone();
            let holder = weak_holder.clone();
            let task = ui
                .runtime
                .spawn(async move { core.set_continue_hidden(&key, true).await });
            let weak = Rc::downgrade(&ui);
            glib::spawn_future_local(async move {
                match task.await {
                    Ok(Ok(_)) => {
                        if let Some(holder) = holder.upgrade() {
                            hide_continue_card(&holder);
                        }
                    }
                    result => {
                        if let Some(button) = button.upgrade() {
                            button.set_sensitive(true);
                        }
                        if let Some(ui) = weak.upgrade() {
                            let message = match result {
                                Ok(Err(error)) => error.message,
                                _ => "Could not remove this title. Try again.".into(),
                            };
                            ui.toast.add_toast(adw::Toast::new(&message));
                        }
                    }
                }
            });
        });
        remove.set_icon_name("window-close-symbolic");
        remove.remove_css_class("compact-action");
        remove.add_css_class("continue-remove");
        remove.set_halign(gtk::Align::End);
        remove.set_valign(gtk::Align::Start);
        remove.set_margin_top(8);
        remove.set_margin_end(8);
        remove.set_tooltip_text(Some("Remove from Continue Watching"));
        remove.update_property(&[gtk::accessible::Property::Label(
            "Remove from Continue Watching",
        )]);
        surface.add_overlay(&remove);
        holder.append(&surface);
    }
    pub(in crate::app) fn episode_browser(
        self: &Rc<Self>,
        key: &ItemKey,
        videos: &[Video],
        progress: &[Progress],
        selected: Option<&str>,
        choose: Rc<dyn Fn(String)>,
    ) -> gtk::Box {
        let content = form();
        let mut seasons: Vec<_> = videos.iter().map(|v| v.season).collect();
        seasons.sort();
        seasons.dedup();
        let names: Vec<String> = seasons
            .iter()
            .map(|s| match s {
                Some(0) => "Specials".into(),
                Some(n) => format!("Season {n}"),
                None => "Episodes".into(),
            })
            .collect();
        let dropdown =
            gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>());
        dropdown.set_halign(gtk::Align::Start);
        dropdown.set_size_request(220, 44);
        dropdown.set_enable_search(true);
        dropdown.set_tooltip_text(Some("Choose or search for a season"));
        content.append(&dropdown);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        // The details page owns scrolling; avoid a small nested episode viewport.
        content.append(&list);
        let current_season = videos
            .iter()
            .find(|v| Some(v.id.as_str()) == selected)
            .and_then(|v| v.season);
        let index = seasons
            .iter()
            .position(|s| *s == current_season)
            .unwrap_or_else(|| seasons.iter().position(|s| *s == Some(1)).unwrap_or(0));
        dropdown.set_selected(index as u32);
        let videos = videos.to_vec();
        let progress = progress.to_vec();
        let key = key.clone();
        let selected = selected.map(str::to_owned);
        let weak = Rc::downgrade(self);
        let weak_list = list.downgrade();
        let render = Rc::new(move |index: usize| {
            let (Some(ui), Some(list)) = (weak.upgrade(), weak_list.upgrade()) else {
                return;
            };
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let season = seasons.get(index).copied().flatten();
            let mut episodes: Vec<_> = videos.iter().filter(|v| v.season == season).collect();
            episodes.sort_by_key(|v| v.episode);
            for video in episodes {
                let row = adw::ActionRow::new();
                row.set_use_markup(false);
                row.set_title(&episode_title(video));
                row.set_title_lines(2);
                let saved = progress
                    .iter()
                    .rev()
                    .find(|p| p.key == key && p.video_id == video.id);
                let available = madari_core::episode_available(video, &today());
                let status = if !available {
                    "Not yet released".into()
                } else {
                    progress_text(saved)
                };
                let description = video
                    .extra
                    .get("overview")
                    .or_else(|| video.extra.get("description"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                row.set_subtitle(&format!("{status}\n{description}"));
                row.set_subtitle_lines(3);
                row.set_activatable(available);
                let thumbnail = gtk::Box::new(gtk::Orientation::Vertical, 5);
                thumbnail.set_valign(gtk::Align::Center);
                let art = gtk::Overlay::new();
                art.set_size_request(112, 63);
                art.add_css_class("poster-art");
                art.set_overflow(gtk::Overflow::Hidden);
                art.set_valign(gtk::Align::Center);
                art.set_child(Some(&gtk::Image::from_icon_name(
                    "video-x-generic-symbolic",
                )));
                if let Some(url) = video.extra.get("thumbnail").and_then(|v| v.as_str()) {
                    let picture = gtk::Picture::new();
                    picture.set_can_shrink(true);
                    picture.set_content_fit(gtk::ContentFit::Cover);
                    art.add_overlay(&picture);
                    ui.artwork(&picture, &art, Some(url), &key.installation_id);
                }
                thumbnail.append(&art);
                if let Some(p) = saved
                    && let Some(duration) = p.duration_ms.filter(|d| *d > 0)
                {
                    thumbnail.append(&viewing_progress(
                        p.position_ms as f64 / duration as f64,
                        112,
                    ));
                }
                row.add_prefix(&thumbnail);
                row.add_suffix(&gtk::Image::from_icon_name(
                    if selected.as_deref() == Some(&video.id) {
                        "media-playback-start-symbolic"
                    } else if saved.is_some_and(|p| p.completed) {
                        "object-select-symbolic"
                    } else {
                        "go-next-symbolic"
                    },
                ));
                let choose = choose.clone();
                let id = video.id.clone();
                row.connect_activated(move |_| choose(id.clone()));
                crate::app::interface::motion::fade_in(&row);
                list.append(&row);
            }
        });
        render(index);
        dropdown.connect_selected_notify(move |d| render(d.selected() as usize));
        content
    }
}

/// Update only the rail; the rest of the dashboard and its scroll position stay intact.
pub(in crate::app) fn hide_continue_card(holder: &gtk::Box) {
    holder.set_visible(false);
    let Some(row) = holder.parent() else {
        return;
    };
    let mut child = row.first_child();
    while let Some(widget) = child {
        if widget.is_visible() {
            return;
        }
        child = widget.next_sibling();
    }
    let mut parent = row.parent();
    while let Some(widget) = parent {
        if widget.has_css_class("continue-section") {
            widget.set_visible(false);
            return;
        }
        parent = widget.parent();
    }
}
