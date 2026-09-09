//! Present addon metadata without exposing raw provider fields.
use super::*;
use madari_model::{ItemKey, Meta, Progress};
use serde_json::Value;

fn values(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => vec![s.trim().into()],
        Value::Number(n) => vec![n.to_string()],
        Value::Array(items) => items.iter().flat_map(values).collect(),
        Value::Object(object) => object.get("name").map(values).unwrap_or_default(),
        _ => vec![],
    }
}
fn field(meta: &Meta, aliases: &[&str]) -> Vec<String> {
    aliases
        .iter()
        .filter_map(|key| meta.extra.get(*key))
        .map(values)
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}
pub fn hero_facts(meta: &Meta) -> Vec<String> {
    let mut facts = Vec::new();
    for aliases in [
        &["releaseInfo", "year"][..],
        &["runtime"][..],
        &["certification", "contentRating", "ageRating"][..],
    ] {
        let text = field(meta, aliases).join(", ");
        if !text.is_empty() {
            facts.push(text);
        }
    }
    if let Some(rating) = field(meta, &["imdbRating"]).first() {
        facts.push(format!("IMDb {rating}/10"));
    }
    let genres = field(meta, &["genres", "genre"]);
    facts.extend(genres.into_iter().take(3));
    facts
}
fn web_link(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}
fn link_button(url: &str, title: &str) -> gtk::LinkButton {
    let link = gtk::LinkButton::with_label(url, title);
    link.set_halign(gtk::Align::Start);
    if let Some(label) = link.child().and_downcast::<gtk::Label>() {
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(45);
    }
    link.set_tooltip_text(Some(title));
    link
}
fn trailers(meta: &Meta) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for key in ["trailers", "trailerStreams"] {
        if let Some(items) = meta.extra.get(key).and_then(Value::as_array) {
            for item in items {
                let url = item
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(web_link)
                    .or_else(|| {
                        let id = item
                            .get("ytId")
                            .or_else(|| item.get("source"))
                            .and_then(Value::as_str)?;
                        if id.len() != 11
                            || !id
                                .bytes()
                                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                        {
                            return None;
                        }
                        Some(format!("https://www.youtube.com/watch?v={id}"))
                    });
                if let Some(url) = url
                    && !result.iter().any(|(_, existing)| existing == &url)
                {
                    let title = item
                        .get("title")
                        .or_else(|| item.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or("Trailer");
                    result.push((title.into(), url));
                }
            }
        }
    }
    result
}
impl Ui {
    pub(in crate::app) fn detail_links(self: &Rc<Self>, actions: &gtk::Box, meta: &Meta) {
        if let Some((_, url)) = trailers(meta).first() {
            let trailer = link_button(url, "Watch trailer ↗");
            trailer.add_css_class("compact-action");
            trailer.set_tooltip_text(Some("Watch trailer in your browser"));
            actions.append(&trailer);
        }
    }
    pub(in crate::app) fn rich_details(
        self: &Rc<Self>,
        key: &ItemKey,
        meta: &Meta,
        progress: &[Progress],
    ) {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 28);
        body.add_css_class("detail-body");
        self.content.append(&body);
        if !meta.videos.is_empty() {
            let episodes = form();
            let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            heading.append(&label("Episodes", "section-title"));
            heading.append(&label(
                &format!("{} episodes", meta.videos.len()),
                "dim-label",
            ));
            episodes.append(&heading);
            let weak = Rc::downgrade(self);
            let play_key = key.clone();
            let recommended = madari_core::continue_episode(
                &meta.videos,
                progress,
                key,
                &crate::app::browsing::episodes::today(),
            )
            .map(|v| v.id.clone());
            episodes.append(&self.episode_browser(
                key,
                &meta.videos,
                progress,
                recommended.as_deref(),
                Rc::new(move |id| {
                    if let Some(ui) = weak.upgrade() {
                        ui.sources(play_key.clone(), id);
                    }
                }),
            ));
            body.append(&episodes);
        }
        let about = gtk::Box::new(gtk::Orientation::Vertical, 24);
        about.add_css_class("detail-about");
        let cast = field(meta, &["cast", "actors"]);
        let has_cast = !cast.is_empty();
        if has_cast {
            let cast_page = gtk::Box::new(gtk::Orientation::Vertical, 16);
            cast_page.add_css_class("detail-about");
            cast_page.append(&label("Cast", "title-2"));
            let chips = gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .column_spacing(10)
                .row_spacing(10)
                .max_children_per_line(6)
                .build();
            for name in cast {
                let card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                card.add_css_class("credit-card");
                card.append(&gtk::Image::from_icon_name("avatar-default-symbolic"));
                let name = label(&name, "heading");
                name.set_max_width_chars(22);
                card.append(&name);
                chips.insert(&card, -1);
            }
            cast_page.append(&chips);
            body.append(&cast_page);
        }
        let info = gtk::Grid::builder()
            .column_spacing(24)
            .row_spacing(14)
            .build();
        let mut row = 0;
        for (title, keys) in [
            ("Director", &["director", "directors"][..]),
            ("Writer", &["writer", "writers"][..]),
            ("Creators", &["creator", "creators"][..]),
            ("Release", &["releaseInfo", "year", "released"][..]),
            ("Runtime", &["runtime"][..]),
            (
                "Rating",
                &["certification", "contentRating", "ageRating"][..],
            ),
            ("IMDb", &["imdbRating"][..]),
            ("Country", &["country", "countries"][..]),
            ("Language", &["language", "languages"][..]),
            ("Status", &["status"][..]),
            ("Network", &["network", "networks"][..]),
            ("Awards", &["awards"][..]),
        ] {
            let mut value = field(meta, keys);
            if title == "Language" {
                value = value
                    .into_iter()
                    .map(|s| crate::app::playback::languages::name(&s))
                    .collect();
            }
            if value.is_empty() {
                continue;
            }
            info.attach(&label(title, "dim-label"), 0, row, 1, 1);
            let text = label(&value.join(", "), "body");
            text.set_selectable(true);
            text.set_hexpand(true);
            text.set_max_width_chars(70);
            info.attach(&text, 1, row, 1, 1);
            row += 1;
        }
        if row > 0 {
            about.append(&label("Details", "title-2"));
            about.append(&info);
        }
        let trailers = trailers(meta);
        if !trailers.is_empty() {
            about.append(&label("Trailers & extras", "title-2"));
            for (title, url) in trailers.into_iter().take(8) {
                let link = link_button(&url, &format!("{title} ↗"));
                link.set_halign(gtk::Align::Start);
                link.set_tooltip_text(Some("Open video in your browser"));
                about.append(&link);
            }
        }
        if let Some(links) = meta.extra.get("links").and_then(Value::as_array) {
            let mut heading_added = false;
            for item in links.iter().take(12) {
                if let (Some(name), Some(url)) = (
                    item.get("name").and_then(Value::as_str),
                    item.get("url").and_then(Value::as_str).and_then(web_link),
                ) {
                    if !heading_added {
                        about.append(&label("Explore more", "title-2"));
                        heading_added = true;
                    }
                    let link = link_button(&url, name);
                    link.set_halign(gtk::Align::Start);
                    about.append(&link);
                }
            }
        }
        body.append(&about);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rich_metadata_handles_aliases_objects_and_numeric_ratings() {
        let meta: Meta = serde_json::from_value(serde_json::json!({
            "id":"tt1","type":"series","name":"Show","genre":["Drama"],"year":"2026",
            "imdbRating":8.2,"director":{"name":"A Director"},"cast":["A",{"name":"B"}],
            "trailers":[{"source":"abcdefghijk","type":"Trailer"},{"url":"file:///secret"}],
            "trailerStreams":[{"ytId":"abcdefghijk"}]
        }))
        .unwrap();
        assert!(hero_facts(&meta).contains(&"IMDb 8.2/10".into()));
        assert!(hero_facts(&meta).contains(&"Drama".into()));
        assert_eq!(field(&meta, &["cast"]), vec!["A", "B"]);
        assert_eq!(field(&meta, &["director"]), vec!["A Director"]);
        assert_eq!(trailers(&meta).len(), 1);
        assert!(web_link("javascript:alert(1)").is_none());
    }
}
