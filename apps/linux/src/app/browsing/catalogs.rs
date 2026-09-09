//! Incremental catalog grids; only one page request is active per collection.
use super::*;
use madari_addon::Catalog;
use madari_model::{ItemKey, Meta, Resource, ResourceData, ResourceRequest};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time::Duration;

#[derive(Default)]
struct Pages {
    offset: usize,
    seen: HashSet<(String, String)>,
}
impl Pages {
    fn append(&mut self, items: Vec<Meta>, paginated: bool) -> (VecDeque<Meta>, bool) {
        let count = items.len();
        self.offset = self.offset.saturating_add(100);
        let fresh: VecDeque<_> = items
            .into_iter()
            .filter(|meta| {
                self.seen
                    .insert((meta.content_type.clone(), meta.id.clone()))
            })
            .collect();
        let exhausted = !paginated || count < 100 || fresh.is_empty();
        (fresh, exhausted)
    }
}
enum Cards {
    Grid(gtk::FlowBox),
    Row(gtk::Box, gtk::ScrolledWindow),
}
impl Cards {
    fn append(&self, widget: &impl IsA<gtk::Widget>) {
        match self {
            Self::Grid(grid) => grid.insert(widget, -1),
            Self::Row(row, _) => row.append(widget),
        }
    }
    fn clear(&self) {
        match self {
            Self::Grid(grid) => {
                while let Some(child) = grid.first_child() {
                    grid.remove(&child);
                }
            }
            Self::Row(row, _) => {
                while let Some(child) = row.first_child() {
                    row.remove(&child);
                }
            }
        }
    }
    fn near_end(&self) -> bool {
        match self {
            Self::Grid(_) => true,
            Self::Row(_, scroll) => {
                let adjustment = scroll.hadjustment();
                adjustment.value() + adjustment.page_size() * 1.5 >= adjustment.upper()
            }
        }
    }
}
struct CatalogGrid {
    provider: String,
    catalog: Catalog,
    extra: BTreeMap<String, String>,
    pages: RefCell<Pages>,
    loading: Cell<bool>,
    exhausted: Cell<bool>,
    failed: Cell<bool>,
    generation: u64,
    cards: Cards,
    status: gtk::Box,
    section: glib::WeakRef<gtk::Box>,
}
impl Ui {
    pub(in crate::app) fn back_button(self: &Rc<Self>) -> gtk::Button {
        let button = gtk::Button::from_icon_name("go-previous-symbolic");
        button.add_css_class("flat");
        button.add_css_class("compact-action");
        button.set_halign(gtk::Align::Start);
        button.set_valign(gtk::Align::Center);
        button.set_tooltip_text(Some("Back to home"));
        button.update_property(&[gtk::accessible::Property::Label("Back to home")]);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                if let Some(task) = ui.request_abort.borrow_mut().take() {
                    task.abort();
                }
                ui.busy.set(false);
                ui.page_loading.set(false);
                ui.content.set_sensitive(true);
                ui.window.set_cursor_from_name(None);
                ui.clear();
                ui.refresh();
            }
        });
        button
    }
    pub(in crate::app) fn catalog_grid(
        self: &Rc<Self>,
        section: &gtk::Box,
        provider: String,
        catalog: Catalog,
        extra: BTreeMap<String, String>,
    ) {
        let cards = if extra.contains_key("search") {
            let (scroll, row) = crate::app::browsing::home::rail();
            scroll.set_hexpand(true);
            let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            navigation.set_halign(gtk::Align::End);
            for (icon, title, direction) in [
                ("go-previous-symbolic", "Previous results", -1.0),
                ("go-next-symbolic", "Next results", 1.0),
            ] {
                let button = gtk::Button::from_icon_name(icon);
                button.add_css_class("flat");
                button.set_tooltip_text(Some(title));
                button.update_property(&[gtk::accessible::Property::Label(title)]);
                let adjustment = scroll.hadjustment();
                button.connect_clicked(move |_| {
                    adjustment.set_value(
                        (adjustment.value() + direction * adjustment.page_size() * 0.85).clamp(
                            adjustment.lower(),
                            (adjustment.upper() - adjustment.page_size()).max(adjustment.lower()),
                        ),
                    );
                });
                navigation.append(&button);
            }
            section.append(&navigation);
            section.append(&scroll);
            Cards::Row(row, scroll)
        } else {
            let grid = gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .homogeneous(true)
                .column_spacing(20)
                .row_spacing(24)
                .max_children_per_line(12)
                .min_children_per_line(1)
                .build();
            grid.add_css_class("poster-grid");
            section.append(&grid);
            Cards::Grid(grid)
        };
        let status = gtk::Box::new(gtk::Orientation::Vertical, 8);
        section.append(&status);
        let state = Rc::new(CatalogGrid {
            provider,
            catalog,
            extra,
            pages: RefCell::new(Pages::default()),
            loading: Cell::new(false),
            exhausted: Cell::new(false),
            failed: Cell::new(false),
            generation: self.view_generation.get(),
            cards,
            status,
            section: section.downgrade(),
        });
        self.load_catalog_grid(state.clone());
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(200), move || {
            let Some(ui) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if ui.view_generation.get() != state.generation || state.section.upgrade().is_none() {
                return glib::ControlFlow::Break;
            }
            if state.exhausted.get() {
                return glib::ControlFlow::Break;
            }
            if !state.loading.get()
                && !state.failed.get()
                && state.cards.near_end()
                && crate::app::interface::loading::in_view(&state.status)
            {
                ui.load_catalog_grid(state.clone());
            }
            glib::ControlFlow::Continue
        });
    }
    fn load_catalog_grid(self: &Rc<Self>, state: Rc<CatalogGrid>) {
        if state.loading.replace(true) {
            return;
        }
        state.failed.set(false);
        while let Some(child) = state.status.first_child() {
            state.status.remove(&child);
        }
        let spinner = gtk::Spinner::new();
        spinner.set_halign(gtk::Align::Start);
        spinner.start();
        state.status.append(&spinner);
        state.status.append(&label("Loading titles…", "dim-label"));
        if state.pages.borrow().offset == 0 {
            for _ in 0..6 {
                state
                    .cards
                    .append(&crate::app::interface::loading::poster_skeleton());
            }
        }
        let Some(core) = self.core.borrow().as_ref().cloned() else {
            return;
        };
        let mut extra = state.extra.clone();
        if state.catalog.paginated() {
            extra.insert("skip".into(), state.pages.borrow().offset.to_string());
        }
        let request = ResourceRequest {
            resource: Resource::Catalog,
            content_type: state.catalog.content_type.clone(),
            id: state.catalog.id.clone(),
            extra,
        };
        let provider = state.provider.clone();
        let task = self
            .runtime
            .spawn(async move { core.query(&provider, request).await });
        // Cancel collection requests on page navigation, just like artwork requests.
        self.artwork_tasks.borrow_mut().push(task.abort_handle());
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = task.await;
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if ui.view_generation.get() != state.generation || state.section.upgrade().is_none() {
                return;
            }
            if state.pages.borrow().offset == 0 {
                state.cards.clear();
            }
            while let Some(child) = state.status.first_child() {
                state.status.remove(&child);
            }
            let items = match result {
                Ok(Ok(ResourceData::Catalog(items))) => items,
                _ => {
                    state.loading.set(false);
                    state.failed.set(true);
                    state.status.append(&label(
                        "Couldn’t load titles. Check your connection and retry.",
                        "dim-label",
                    ));
                    let retry = Rc::downgrade(&state);
                    state.status.append(&ui.button("Retry", move |ui| {
                        if let Some(state) = retry.upgrade() {
                            ui.load_catalog_grid(state);
                        }
                    }));
                    return;
                }
            };
            let (mut pending, exhausted) = state
                .pages
                .borrow_mut()
                .append(items, state.catalog.paginated());
            state.exhausted.set(exhausted);
            let weak = Rc::downgrade(&ui);
            // Yield between small batches so scrolling and animation stay responsive.
            glib::timeout_add_local(Duration::from_millis(16), move || {
                let Some(ui) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if ui.view_generation.get() != state.generation || state.section.upgrade().is_none()
                {
                    return glib::ControlFlow::Break;
                }
                for _ in 0..8 {
                    let Some(meta) = pending.pop_front() else {
                        break;
                    };
                    let key = ItemKey {
                        installation_id: state.provider.clone(),
                        content_type: meta.content_type.clone(),
                        item_id: meta.id.clone(),
                    };
                    let card = ui.poster(key, meta);
                    crate::app::interface::motion::fade_in(&card);
                    state.cards.append(&card);
                }
                if !pending.is_empty() {
                    return glib::ControlFlow::Continue;
                }
                state.loading.set(false);
                if state.pages.borrow().seen.is_empty() {
                    state.status.append(&label("No titles found.", "dim-label"));
                } else if state.exhausted.get() {
                    state
                        .status
                        .append(&label("All titles loaded.", "dim-label"));
                } else {
                    state.status.append(&label("Scroll for more", "dim-label"));
                }
                glib::ControlFlow::Break
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn meta(id: &str) -> Meta {
        serde_json::from_value(serde_json::json!({"id":id,"type":"movie","name":id})).unwrap()
    }
    #[test]
    fn paging_uses_standard_offsets_and_stops_repeated_pages() {
        let mut pages = Pages::default();
        let batch = || (0..100).map(|i| meta(&format!("id{i}"))).collect();
        assert!(!pages.append(batch(), true).1);
        assert_eq!(pages.offset, 100);
        assert!(pages.append(batch(), true).1);
        assert_eq!(pages.offset, 200);
    }
    #[test]
    fn short_pages_and_unpaginated_catalogs_stop() {
        assert!(Pages::default().append(vec![meta("a")], true).1);
        assert!(Pages::default().append(vec![], true).1);
        assert!(Pages::default().append(vec![meta("a")], false).1);
    }
}
