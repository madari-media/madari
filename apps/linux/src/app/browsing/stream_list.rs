//! Shared source presentation for browsing and the in-player source picker.
use adw::prelude::*;
use madari_model::Stream;

fn line(value: &str, style: &str, lines: i32) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_lines(lines);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_hexpand(true);
    label.set_max_width_chars(52);
    label.add_css_class(style);
    label
}

pub fn content(stream: &Stream, selected: bool) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    card.add_css_class("source-content");
    let icon = gtk::Image::from_icon_name(if selected {
        "object-select-symbolic"
    } else {
        "media-playback-start-symbolic"
    });
    icon.set_pixel_size(22);
    icon.set_valign(gtk::Align::Center);
    card.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 7);
    text.set_hexpand(true);
    let header = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let name = stream.name.as_deref().unwrap_or("Video source");
    let mut names = name.lines().filter(|s| !s.trim().is_empty());
    header.append(&line(names.next().unwrap_or("Video source"), "heading", 1));
    let quality = names.collect::<Vec<_>>().join(" · ");
    if !quality.is_empty() {
        let badge = line(&quality, "source-badge", 1);
        badge.set_hexpand(false);
        badge.set_max_width_chars(18);
        header.append(&badge);
    }
    text.append(&header);
    let title = stream.title.as_deref().unwrap_or("");
    let mut details = title.lines().filter(|s| !s.trim().is_empty());
    if let Some(title) = details.next() {
        text.append(&line(title, "source-title", 2));
    }
    let details = details.collect::<Vec<_>>().join(" · ");
    if !details.is_empty() {
        text.append(&line(&details, "dim-label", 2));
    }
    let torrent = stream.info_hash.is_some()
        || stream
            .url
            .as_deref()
            .is_some_and(|u| u.starts_with("magnet:"));
    text.append(&line(
        if selected {
            "Now playing"
        } else if torrent {
            "Torrent"
        } else if stream.url.is_some() {
            "Direct stream"
        } else {
            "External source"
        },
        "source-kind",
        1,
    ));
    card.append(&text);
    let filename = stream
        .behavior_hints
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    card.set_tooltip_text(Some(format!("{name}\n{title}\n{filename}").trim()));
    card
}

/// Filter existing widgets by installation ID, retaining their original actions and order.
pub fn addon_filter(groups: Vec<(String, String, usize, gtk::Widget)>) -> gtk::ScrolledWindow {
    let mut addons = Vec::<(String, String, usize)>::new();
    for (id, name, count, _) in &groups {
        if let Some(addon) = addons.iter_mut().find(|addon| &addon.0 == id) {
            addon.2 += count;
        } else {
            addons.push((id.clone(), name.clone(), *count));
        }
    }
    let total: usize = addons.iter().map(|a| a.2).sum();
    let mut choices = vec![(None, "All addons".to_owned(), total)];
    for (index, (id, name, count)) in addons.iter().enumerate() {
        let duplicate = addons[..index].iter().filter(|a| &a.1 == name).count();
        let name = if duplicate == 0 {
            name.clone()
        } else {
            format!("{name} ({})", duplicate + 1)
        };
        choices.push((Some(id.clone()), name, *count));
    }
    let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    tabs.add_css_class("addon-tabs");
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .hexpand(true)
        .child(&tabs)
        .build();
    scroll.update_property(&[gtk::accessible::Property::Label("Filter streams by addon")]);
    let groups = std::rc::Rc::new(groups);
    let mut first: Option<gtk::ToggleButton> = None;
    for (selected, name, count) in choices {
        let button = gtk::ToggleButton::new();
        button.add_css_class("addon-tab");
        button.set_group(first.as_ref());
        button.set_tooltip_text(Some(&format!("{name} · {count} streams")));
        button.update_property(&[gtk::accessible::Property::Label(&format!(
            "{name}, {count} streams"
        ))]);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some(&name));
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(20);
        let badge = gtk::Label::new(Some(&count.to_string()));
        badge.add_css_class("addon-count");
        content.append(&title);
        content.append(&badge);
        button.set_child(Some(&content));
        if first.is_none() {
            button.set_active(true);
            first = Some(button.clone());
        }
        let groups = groups.clone();
        let weak_tabs = tabs.downgrade();
        let adjustment = scroll.hadjustment();
        button.connect_toggled(move |button| {
            if !button.is_active() {
                return;
            }
            for (id, _, _, widget) in groups.iter() {
                widget.set_visible(selected.as_ref().is_none_or(|selected| selected == id));
            }
            if adjustment.page_size() > 0.0
                && let Some(tabs) = weak_tabs.upgrade()
                && let Some(bounds) = button.compute_bounds(&tabs)
            {
                adjustment.clamp_page(bounds.x() as f64, (bounds.x() + bounds.width()) as f64);
            }
        });
        tabs.append(&button);
    }
    scroll
}
