//! Built-in background transfers, independent of the player window.
use super::*;
use std::{collections::BTreeMap, time::Duration};

pub(in crate::app) struct TransferRow {
    pub(in crate::app) root: gtk::Box,
    state: gtk::Label,
    progress: gtk::ProgressBar,
    size: gtk::Label,
    download: gtk::Label,
    upload: gtk::Label,
    peers: gtk::Label,
    pause: gtk::Button,
    files: gtk::Button,
    paused: Rc<Cell<bool>>,
}
impl TransferRow {
    pub(in crate::app) fn new(name: &str) -> (Self, gtk::Button, gtk::Box) {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("transfer-row");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let icon = gtk::Image::from_icon_name("folder-download-symbolic");
        icon.add_css_class("transfer-icon");
        icon.set_pixel_size(22);
        header.append(&icon);
        let title = label(name, "transfer-title");
        title.set_wrap(false);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_width_chars(1);
        title.set_hexpand(true);
        title.set_tooltip_text(Some(name));
        header.append(&title);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let pause = gtk::Button::from_icon_name("media-playback-pause-symbolic");
        let files = gtk::Button::from_icon_name("folder-open-symbolic");
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        for (button, title) in [
            (&files, "Open in Files"),
            (&pause, "Pause torrent"),
            (&remove, "Remove torrent · keep files"),
        ] {
            button.add_css_class("flat");
            button.set_tooltip_text(Some(title));
            button.update_property(&[gtk::accessible::Property::Label(title)]);
            actions.append(button);
        }
        header.append(&actions);
        root.append(&header);
        let detail = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let state = label("", "transfer-state");
        state.set_halign(gtk::Align::Start);
        detail.append(&state);
        let size = label("", "transfer-size");
        size.set_hexpand(true);
        size.set_xalign(1.0);
        detail.append(&size);
        root.append(&detail);
        let progress = gtk::ProgressBar::new();
        progress.add_css_class("transfer-progress");
        progress.update_property(&[gtk::accessible::Property::Label("Video download progress")]);
        root.append(&progress);
        let metrics = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        metrics.set_homogeneous(true);
        let mut values = Vec::new();
        for caption in ["Download", "Upload", "Peers"] {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
            column.append(&label(caption, "transfer-caption"));
            let value = label("—", "transfer-value");
            column.append(&value);
            values.push(value);
            metrics.append(&column);
        }
        root.append(&metrics);
        let row = Self {
            root,
            state,
            progress,
            size,
            download: values[0].clone(),
            upload: values[1].clone(),
            peers: values[2].clone(),
            pause,
            files,
            paused: Rc::new(Cell::new(false)),
        };
        (row, remove, actions)
    }
    pub(in crate::app) fn update(&self, stats: &madari_native::internal_media::TorrentStats) {
        let paused = stats.state == "Paused";
        self.paused.set(paused);
        self.pause.set_icon_name(if paused {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });
        let action = if paused {
            "Resume torrent"
        } else {
            "Pause torrent"
        };
        self.pause.set_tooltip_text(Some(action));
        self.pause
            .update_property(&[gtk::accessible::Property::Label(action)]);
        self.state.set_text(&stats.state);
        for (class, active) in [
            ("seeding", stats.state == "Seeding"),
            ("paused", paused),
            ("failed", stats.state == "Error"),
        ] {
            if active {
                self.root.add_css_class(class);
            } else {
                self.root.remove_css_class(class);
            }
        }
        let fraction = if stats.total > 0 {
            (stats.downloaded as f64 / stats.total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.progress.set_fraction(fraction);
        self.size.set_text(&format!(
            "{} / {} · {:.0}%",
            bytes(stats.downloaded),
            bytes(stats.total),
            fraction * 100.0
        ));
        self.download
            .set_text(&format!("{}/s", bytes(stats.download_bytes_per_second)));
        self.upload
            .set_text(&format!("{}/s", bytes(stats.upload_bytes_per_second)));
        self.peers.set_text(&stats.peers.to_string());
    }
}
fn bytes(value: u64) -> String {
    if value >= 1073741824 {
        format!("{:.1} GiB", value as f64 / 1073741824.0)
    } else if value >= 1048576 {
        format!("{:.1} MiB", value as f64 / 1048576.0)
    } else {
        format!("{:.0} KiB", value as f64 / 1024.0)
    }
}
impl Ui {
    pub(in crate::app) fn torrent_manager(self: &Rc<Self>) {
        let dialog = adw::Dialog::builder()
            .title("Torrents")
            .content_width(760)
            .content_height(620)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let content = form();
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.set_margin_top(12);
        content.set_margin_bottom(16);
        content.append(&label("Downloads & seeding", "section-title"));
        let summary = label("Your transfers continue while Madari is open.", "dim-label");
        content.append(&summary);
        let list = form();
        let empty = label("Loading torrents…", "dim-label");
        content.append(&empty);
        content.append(&list);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .build();
        toolbar.set_content(Some(&scroll));
        dialog.set_child(Some(&toolbar));
        let parent: gtk::Widget = self
            .preferences
            .borrow()
            .as_ref()
            .map(|p| p.clone().upcast())
            .unwrap_or_else(|| self.window.clone().upcast());
        dialog.present(Some(&parent));
        let closed = Rc::new(Cell::new(false));
        let done = closed.clone();
        dialog.connect_closed(move |_| done.set(true));
        let media = self.internal_companion.local().unwrap();
        let start = media.clone();
        let task = self.runtime.spawn(async move { start.start().await });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = task.await;
            if closed.get() {
                return;
            }
            if !matches!(&result, Ok(Ok(()))) {
                empty.set_text(&match result {
                    Ok(Err(error)) => error.message,
                    _ => "Could not start torrent management.".into(),
                });
                return;
            }
            let rows = RefCell::new(BTreeMap::<String, TransferRow>::new());
            let refresh = move || {
                let Some(ui) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if closed.get() {
                    return glib::ControlFlow::Break;
                }
                let torrents = media.managed();
                let active = torrents
                    .iter()
                    .filter(|t| t.stats.state == "Downloading")
                    .count();
                let seeding = torrents
                    .iter()
                    .filter(|t| t.stats.state == "Seeding")
                    .count();
                summary.set_text(&format!(
                    "{} torrents · {active} downloading · {seeding} seeding",
                    torrents.len()
                ));
                empty.set_text("No torrents yet. Start a torrent video to add it here.");
                empty.set_visible(torrents.is_empty());
                let mut rows = rows.borrow_mut();
                rows.retain(|id, row| {
                    let keep = torrents.iter().any(|t| &t.id == id);
                    if !keep {
                        list.remove(&row.root);
                    }
                    keep
                });
                for torrent in torrents {
                    let row = rows.entry(torrent.id.clone()).or_insert_with(|| {
                        let (row, remove, actions) = TransferRow::new(&torrent.name);
                        let directory = torrent.download_directory.clone();
                        let weak = Rc::downgrade(&ui);
                        row.files.connect_clicked(move |_| {
                            let Some(ui) = weak.upgrade() else {
                                return;
                            };
                            let file = gtk::gio::File::for_path(&directory);
                            let launcher = gtk::FileLauncher::new(Some(&file));
                            glib::spawn_future_local(async move {
                                if launcher.launch_future(Some(&ui.window)).await.is_err() {
                                    ui.toast.add_toast(adw::Toast::new(
                                        "Could not open the download folder in Files.",
                                    ));
                                }
                            });
                        });
                        let pause = row.pause.clone();
                        let paused = row.paused.clone();
                        for (button, removing) in [(&pause, false), (&remove, true)] {
                            let weak = Rc::downgrade(&ui);
                            let media = media.clone();
                            let id = torrent.id.clone();
                            let paused = paused.clone();
                            let actions = actions.downgrade();
                            button.connect_clicked(move |_| {
                                let Some(ui) = weak.upgrade() else {
                                    return;
                                };
                                if let Some(actions) = actions.upgrade() {
                                    actions.set_sensitive(false);
                                }
                                let media = media.clone();
                                let id = id.clone();
                                let next_paused = !paused.get();
                                let task = ui.runtime.spawn(async move {
                                    if removing {
                                        media.remove(&id).await
                                    } else {
                                        media.set_paused(&id, next_paused).await
                                    }
                                });
                                let weak = Rc::downgrade(&ui);
                                let actions = actions.clone();
                                glib::spawn_future_local(async move {
                                    let result = task.await;
                                    if let Some(actions) = actions.upgrade() {
                                        actions.set_sensitive(true);
                                    }
                                    if let Some(ui) = weak.upgrade() {
                                        let message = match result {
                                            Ok(Ok(())) => return,
                                            Ok(Err(e)) => e.message,
                                            Err(_) => "Could not update torrent.".into(),
                                        };
                                        ui.toast.add_toast(adw::Toast::new(&message));
                                    }
                                });
                            });
                        }
                        list.append(&row.root);
                        row
                    });
                    row.update(&torrent.stats);
                }
                glib::ControlFlow::Continue
            };
            if refresh() == glib::ControlFlow::Continue {
                glib::timeout_add_local(Duration::from_secs(1), refresh);
            }
        });
    }
}
