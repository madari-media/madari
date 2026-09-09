use super::*;
use madari_addon::Catalog;
use std::collections::BTreeMap;

#[derive(Clone)]
struct Page {
    provider: String,
    catalog: Catalog,
    term: String,
}
impl Ui {
    pub(in crate::app) fn search_page(self: &Rc<Self>, snapshot: PublicSnapshot) {
        self.clear();
        self.section_heading("Search", None);
        let bar = responsive::row(12);
        let input = gtk::SearchEntry::builder()
            .placeholder_text("Search movies and shows")
            .hexpand(true)
            .build();
        input.add_css_class("search-input");
        bar.append(&input);
        let results = form();
        let weak = Rc::downgrade(self);
        let result_box = results.downgrade();
        let search_snapshot = snapshot.clone();
        input.connect_activate(move |input| {
            if let (Some(ui), Some(results)) = (weak.upgrade(), result_box.upgrade()) {
                ui.search_results(&results, &search_snapshot, input.text().trim());
            }
        });
        let result_box = results.downgrade();
        let text = input.clone();
        let search_snapshot = snapshot.clone();
        let button = self.button("Search", move |ui| {
            if let Some(results) = result_box.upgrade() {
                ui.search_results(&results, &search_snapshot, text.text().trim());
            }
        });
        button.add_css_class("play-button");
        bar.append(&button);
        self.content.append(&bar);
        results.append(&label(
            "Search across the catalogs provided by your enabled addons.",
            "hero-meta",
        ));
        self.content.append(&results);
        input.grab_focus();
    }
    fn search_results(self: &Rc<Self>, results: &gtk::Box, snapshot: &PublicSnapshot, term: &str) {
        crate::app::interface::motion::page_change(&self.page_surface);
        while let Some(child) = results.first_child() {
            results.remove(&child);
        }
        if term.is_empty() {
            results.append(&label("Enter a movie or show title.", "hero-meta"));
            return;
        }
        let mut count = 0;
        for addon in snapshot.addons.iter().filter(|a| a.enabled) {
            for catalog in addon.manifest.catalogs.iter().filter(|c| c.searchable()) {
                count += 1;
                results.append(&label(
                    &format!(
                        "{} · {}",
                        catalog.name.as_deref().unwrap_or(&catalog.id),
                        addon.manifest.name
                    ),
                    "section-title",
                ));
                let section = form();
                results.append(&section);
                if catalog
                    .extra
                    .iter()
                    .any(|e| e.is_required && e.name != "search" && e.name != "skip")
                {
                    section.append(&label(
                        "This catalog needs additional filters before searching.",
                        "hero-meta",
                    ));
                    let id = addon.installation_id.clone();
                    let cat = catalog.clone();
                    let term = term.to_owned();
                    section.append(&self.button("Choose filters", move |ui| {
                        ui.catalog_filters(id.clone(), cat.clone(), Some(term.clone()))
                    }));
                    continue;
                }
                self.search_section(
                    &section,
                    Page {
                        provider: addon.installation_id.clone(),
                        catalog: catalog.clone(),
                        term: term.into(),
                    },
                );
            }
        }
        if count == 0 {
            results.append(&label(
                "None of your enabled addons provide searchable catalogs.",
                "hero-meta",
            ));
        }
    }
    fn search_section(self: &Rc<Self>, section: &gtk::Box, page: Page) {
        self.catalog_grid(
            section,
            page.provider,
            page.catalog,
            BTreeMap::from([("search".into(), page.term)]),
        );
    }
}
