use super::*;

const LANGUAGES: &[&str] = &[
    "en", "hi", "ta", "te", "ml", "kn", "mr", "bn", "gu", "pa", "ur", "fr", "de", "es", "pt", "it",
    "ja", "ko", "zh", "ar", "ru", "uk", "nl", "pl", "tr", "sv", "da", "fi", "no", "th", "vi", "id",
    "he",
];

type Changed = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

fn ordered_languages(
    title: &str,
    values: Rc<RefCell<Vec<String>>>,
    changed: Changed,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title)
        .description("First available language wins. Move languages up or down to set their priority. An empty list uses the video's default.")
        .build();
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    group.add(&list);
    fn render(list: &gtk::ListBox, values: &Rc<RefCell<Vec<String>>>, changed: &Changed) {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        let languages = values.borrow().clone();
        for (index, language) in languages.iter().enumerate() {
            let row = adw::ActionRow::builder()
                .use_markup(false)
                .title(crate::app::playback::languages::name(language))
                .subtitle(format!("Preference {}", index + 1))
                .build();
            for (icon, tip, change) in [
                ("go-up-symbolic", "Move earlier", -1),
                ("go-down-symbolic", "Move later", 1),
                ("edit-delete-symbolic", "Remove language", 0),
            ] {
                let button = gtk::Button::from_icon_name(icon);
                button.add_css_class("flat");
                button.set_tooltip_text(Some(tip));
                button.set_valign(gtk::Align::Center);
                button.set_sensitive(
                    change == 0
                        || (change < 0 && index > 0)
                        || (change > 0 && index + 1 < languages.len()),
                );
                let weak = list.downgrade();
                let values = values.clone();
                let changed = changed.clone();
                button.connect_clicked(move |_| {
                    let mut v = values.borrow_mut();
                    if index >= v.len() {
                        return;
                    }
                    if change == 0 {
                        v.remove(index);
                    } else {
                        let other = (index as i32 + change) as usize;
                        if other < v.len() {
                            v.swap(index, other);
                        }
                    }
                    drop(v);
                    if let Some(list) = weak.upgrade() {
                        render(&list, &values, &changed);
                    }
                    if let Some(changed) = changed.borrow().as_ref() {
                        changed();
                    }
                });
                row.add_suffix(&button);
            }
            list.append(&row);
        }
    }
    render(&list, &values, &changed);
    let add = gtk::MenuButton::builder()
        .label("Add language")
        .halign(gtk::Align::Start)
        .build();
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_width(220)
        .max_content_height(280)
        .propagate_natural_height(true)
        .child(&choices)
        .build();
    popover.set_child(Some(&scroll));
    add.set_popover(Some(&popover));
    let weak_choices = choices.downgrade();
    let weak_list = list.downgrade();
    popover.connect_show(move |popover| {
        let Some(choices) = weak_choices.upgrade() else {
            return;
        };
        while let Some(child) = choices.first_child() {
            choices.remove(&child);
        }
        for code in LANGUAGES {
            let selected = values.borrow().iter().any(|v| v == code);
            let name = crate::app::playback::languages::name(code);
            let button = gtk::Button::with_label(&if selected {
                format!("{name} ✓")
            } else {
                name
            });
            button.add_css_class("flat");
            button.set_sensitive(!selected && values.borrow().len() < 20);
            let values = values.clone();
            let changed = changed.clone();
            let weak_list = weak_list.clone();
            let weak_popover = popover.downgrade();
            button.connect_clicked(move |_| {
                let mut v = values.borrow_mut();
                if v.len() >= 20 || v.iter().any(|v| v == code) {
                    return;
                }
                v.push((*code).into());
                drop(v);
                if let Some(popover) = weak_popover.upgrade() {
                    popover.popdown();
                }
                if let Some(list) = weak_list.upgrade() {
                    render(&list, &values, &changed);
                }
                if let Some(changed) = changed.borrow().as_ref() {
                    changed();
                }
            });
            choices.append(&button);
        }
    });
    group.add(&add);
    group
}

fn track_option(title: &str, value: madari_model::TrackPreference) -> adw::ComboRow {
    use madari_model::TrackPreference::*;
    adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(&[
            "No preference",
            "Prefer",
            "Avoid when alternatives exist",
        ]))
        .selected(match value {
            Any => 0,
            Prefer => 1,
            Avoid => 2,
        })
        .build()
}
fn choice(row: &adw::ComboRow) -> madari_model::TrackPreference {
    use madari_model::TrackPreference::*;
    match row.selected() {
        1 => Prefer,
        2 => Avoid,
        _ => Any,
    }
}

impl Ui {
    pub(in crate::app) fn playback_preferences_page(
        self: &Rc<Self>,
        preferences: madari_model::PlaybackPreferences,
    ) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::builder()
            .title("Playback")
            .name("playback")
            .icon_name("media-playback-start-symbolic")
            .build();
        let audio = Rc::new(RefCell::new(preferences.audio_languages));
        let subtitles = Rc::new(RefCell::new(preferences.subtitle_languages));
        let changed: Changed = Rc::new(RefCell::new(None));
        page.add(&ordered_languages(
            "Audio languages",
            audio.clone(),
            changed.clone(),
        ));
        page.add(&ordered_languages(
            "Subtitle languages",
            subtitles.clone(),
            changed.clone(),
        ));
        let options = adw::PreferencesGroup::builder()
            .title("Defaults for this profile")
            .description("Changes save automatically and apply when opening a video. You can still change tracks in the player.")
            .build();
        let enabled = adw::SwitchRow::builder()
            .title("Enable subtitles by default")
            .active(preferences.subtitles_enabled)
            .build();
        options.add(&enabled);
        let sdh = track_option(
            "SDH subtitles · dialogue and sound descriptions",
            preferences.subtitle_sdh,
        );
        let forced = track_option(
            "Forced subtitles · translated foreign dialogue",
            preferences.subtitle_forced,
        );
        let description = track_option(
            "Audio description · narrated visual action",
            preferences.audio_description,
        );
        let commentary = track_option("Audio commentary", preferences.audio_commentary);
        for row in [&sdh, &forced, &description, &commentary] {
            options.add(row);
        }

        let weak = Rc::downgrade(self);
        let weak_enabled = enabled.downgrade();
        let rows = [
            sdh.downgrade(),
            forced.downgrade(),
            description.downgrade(),
            commentary.downgrade(),
        ];
        *changed.borrow_mut() = Some(Rc::new(move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(enabled) = weak_enabled.upgrade() else {
                return;
            };
            let rows = rows.iter().filter_map(|r| r.upgrade()).collect::<Vec<_>>();
            if rows.len() != 4 {
                return;
            }
            let preferences = madari_model::PlaybackPreferences {
                subtitle_sdh: choice(&rows[0]),
                subtitle_forced: choice(&rows[1]),
                audio_description: choice(&rows[2]),
                audio_commentary: choice(&rows[3]),
                audio_languages: audio.borrow().clone(),
                subtitle_languages: subtitles.borrow().clone(),
                subtitles_enabled: enabled.is_active(),
            };
            let core = ui.core.borrow().as_ref().unwrap().clone();
            ui.run(
                async move { core.set_playback_preferences(preferences).await },
                |_, _| {},
            );
        }));
        for row in [&sdh, &forced, &description, &commentary] {
            let changed = changed.clone();
            row.connect_selected_notify(move |_| {
                if let Some(changed) = changed.borrow().as_ref() {
                    changed();
                }
            });
        }
        let changed_enabled = changed.clone();
        enabled.connect_active_notify(move |_| {
            if let Some(changed) = changed_enabled.borrow().as_ref() {
                changed();
            }
        });
        page.add(&options);
        page
    }
}
