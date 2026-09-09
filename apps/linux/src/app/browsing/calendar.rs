use super::*;

fn release_date(video: &madari_model::Video) -> Option<glib::DateTime> {
    let value = video.extra.get("released")?.as_str()?;
    let value = if value.len() == 10 {
        format!("{value}T00:00:00Z")
    } else {
        value.into()
    };
    glib::DateTime::from_iso8601(&value, None)
        .ok()?
        .to_utc()
        .ok()
}
impl Ui {
    pub(in crate::app) fn calendar_page(self: &Rc<Self>) {
        let Some(core) = self.core.borrow().as_ref().cloned() else {
            return;
        };
        self.loading_page(false);
        self.select_section("Calendar");
        *self.page_retry.borrow_mut() = Some(Rc::new(|ui| ui.calendar_page()));
        self.run(async move { Ok((core.calendar().await?, core.snapshot().await?)) }, |ui, (data, snapshot)| {
            ui.clear();
            let heading = ui.section_heading("Calendar", Some("Episode releases for titles in My list"));
            let reload = ui.button("Refresh", |ui| ui.calendar_page());
            reload.set_icon_name("view-refresh-symbolic");
            reload.add_css_class("flat");
            reload.set_tooltip_text(Some("Refresh calendar"));
            heading.append(&reload);
            if !data.supported {
                ui.content.append(&label("None of your enabled addons provides a calendar. Install an addon that declares calendarVideosIds.", "empty-state"));
                return;
            }
            if snapshot.library.is_empty() {
                ui.content.append(&label("Add shows to My list to follow their episode releases here.", "empty-state"));
                return;
            }
            for warning in &data.warnings { ui.content.append(&label(warning, "dim-label")); }
            for (_, metadata) in &data.titles { ui.remember_metadata(metadata); }
            let calendar = gtk::Calendar::new();
            calendar.set_size_request(-1, 280);
            calendar.set_valign(gtk::Align::Start);
            calendar.add_css_class("release-calendar");
            let sidebar = form();
            sidebar.append(&calendar);
            let overview = gtk::Button::with_label("Show month overview");
            overview.add_css_class("flat");
            sidebar.append(&overview);
            let today = gtk::Button::with_label("Today");
            today.add_css_class("flat");
            sidebar.append(&today);
            let agenda = form();
            agenda.set_hexpand(true);
            let layout = responsive::row(28);
            layout.append(&sidebar);
            layout.append(&agenda);
            ui.content.append(&layout);
            let full_month = Rc::new(Cell::new(true));
            let weak = Rc::downgrade(&ui);
            let weak_calendar = calendar.downgrade();
            let weak_agenda = agenda.downgrade();
            let mode = full_month.clone();
            let render: Rc<dyn Fn()> = Rc::new(move || {
                let (Some(ui), Some(calendar), Some(agenda)) = (weak.upgrade(), weak_calendar.upgrade(), weak_agenda.upgrade()) else { return };
                while let Some(child) = agenda.first_child() { agenda.remove(&child); }
                calendar.clear_marks();
                let selected = calendar.date();
                let heading = selected.format(if mode.get() { "%B %Y" } else { "%A, %e %B" }).map(|s| s.to_string()).unwrap_or_default();
                agenda.append(&label(&heading, "title-2"));
                let mut releases = Vec::new();
                for (key, metadata) in &data.titles {
                    for video in &metadata.meta.videos {
                        let Some(date) = release_date(video) else { continue };
                        if date.year() != selected.year() || date.month() != selected.month() { continue; }
                        calendar.mark_day(date.day_of_month() as u32);
                        if mode.get() || date.day_of_month() == selected.day_of_month() {
                            releases.push((date, key, metadata, video));
                        }
                    }
                }
                releases.sort_by_key(|(date, _, _, video)| (date.to_unix(), video.season, video.episode));
                if releases.is_empty() {
                    agenda.append(&label("No episode releases reported for this period.", "empty-state"));
                }
                for (date, key, metadata, video) in releases {
                    let row = adw::ActionRow::new();
                    row.set_use_markup(false);
                    row.set_title(&metadata.meta.name);
                    row.set_title_lines(1);
                    let day = date.format("%a, %e %b").map(|s| s.to_string()).unwrap_or_default();
                    row.set_subtitle(&format!("{day} · {}", crate::app::browsing::episodes::episode_title(video)));
                    row.set_subtitle_lines(2);
                    row.set_activatable(true);
                    let weak = Rc::downgrade(&ui);
                    let item = key.clone();
                    row.connect_activated(move |_| {
                        if let Some(ui) = weak.upgrade() {
                            let preview = ui.playback_metadata.borrow().get(&(item.content_type.clone(), item.item_id.clone())).cloned();
                            ui.details(item.clone(), preview);
                        }
                    });
                    let saved = snapshot.progress.iter().rev().find(|p| p.key == *key && p.video_id == video.id);
                    if saved.is_some_and(|p| p.completed) {
                        let watched = gtk::Image::from_icon_name("object-select-symbolic");
                        watched.set_tooltip_text(Some("Watched"));
                        row.add_suffix(&watched);
                    }
                    if madari_core::episode_available(video, &crate::app::browsing::episodes::today()) {
                        let key = key.clone();
                        let id = video.id.clone();
                        let play = ui.button("Play episode", move |ui| ui.resume_saved(key.clone(), id.clone()));
                        play.set_icon_name("media-playback-start-symbolic");
                        play.set_tooltip_text(Some("Play episode"));
                        row.add_suffix(&play);
                    }
                    let list = gtk::ListBox::new();
                    list.set_selection_mode(gtk::SelectionMode::None);
                    list.add_css_class("boxed-list");
                    list.append(&row);
                    agenda.append(&list);
                }
            });
            render();
            let update = render.clone();
            let mode = full_month.clone();
            calendar.connect_day_selected(move |_| { mode.set(false); update(); });
            let update = render.clone();
            let mode = full_month.clone();
            calendar.connect_month_notify(move |_| { mode.set(true); update(); });
            let update = render.clone();
            calendar.connect_year_notify(move |_| { update(); });
            let update = render.clone();
            overview.connect_clicked(move |_| { full_month.set(true); update(); });
            let weak = calendar.downgrade();
            today.connect_clicked(move |_| {
                if let (Some(calendar), Ok(today)) = (weak.upgrade(), glib::DateTime::now_local()) { calendar.select_day(&today); }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_dates_handle_date_only_and_timezone_boundaries() {
        let video = |date| {
            serde_json::from_value(serde_json::json!({"id":"episode", "released":date})).unwrap()
        };
        let date = release_date(&video("2026-01-01T00:30:00+02:00")).unwrap();
        assert_eq!(
            (date.year(), date.month(), date.day_of_month()),
            (2025, 12, 31)
        );
        assert_eq!(
            release_date(&video("2024-02-29")).unwrap().day_of_month(),
            29
        );
        assert!(release_date(&video("invalid")).is_none());
    }
}
