//! Short, frame-clock driven transitions. No permanent animation timer or layout tweening.
use adw::prelude::*;
use gtk::glib;
use std::cell::Cell;

pub fn enabled() -> bool {
    gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations())
}

pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub fn fade_in(widget: &impl IsA<gtk::Widget>) {
    if !enabled() {
        return;
    }
    let widget = widget.as_ref();
    widget.set_opacity(0.0);
    let start = Cell::new(None);
    widget.add_tick_callback(move |widget, clock| {
        let now = clock.frame_time();
        let since = start.get().unwrap_or(now);
        start.set(Some(since));
        let t = ((now - since) as f64 / 180_000.0).min(1.0);
        widget.set_opacity(ease(t));
        if t >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

pub fn slide_in(widget: &impl IsA<gtk::Widget>, delay_ms: u64) -> gtk::Revealer {
    let reveal = gtk::Revealer::builder()
        .child(widget)
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .transition_duration(260)
        .reveal_child(!enabled())
        .build();
    let started = Cell::new(false);
    reveal.connect_map(move |reveal| {
        if started.replace(true) {
            return;
        }
        if !enabled() {
            reveal.set_reveal_child(true);
            return;
        }
        let weak = reveal.downgrade();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(delay_ms.min(180)),
            move || {
                if let Some(reveal) = weak.upgrade() {
                    reveal.set_reveal_child(true);
                }
            },
        );
    });
    reveal
}

/// Preserve the outgoing viewport while the replacement page is built underneath.
/// Only the captured image fades; the new page is immediately interactive.
pub fn page_change(host: &gtk::Overlay) {
    let Some(child) = host.child() else { return };
    while let Some(old) = child.next_sibling() {
        host.remove_overlay(&old);
    }
    if !enabled() || !host.is_mapped() || child.width() <= 0 || child.height() <= 0 {
        return;
    }
    let snapshot = gtk::Snapshot::new();
    let paintable = gtk::WidgetPaintable::new(Some(&child));
    gtk::gdk::prelude::PaintableExt::snapshot(
        &paintable,
        &snapshot,
        child.width() as f64,
        child.height() as f64,
    );
    let Some(image) = snapshot.to_paintable(None) else {
        return;
    };
    let cover = gtk::Picture::for_paintable(&image);
    cover.set_can_shrink(true);
    cover.set_content_fit(gtk::ContentFit::Fill);
    cover.set_can_target(false);
    host.add_overlay(&cover);
    let weak = host.downgrade();
    let start = Cell::new(None);
    cover.add_tick_callback(move |cover, clock| {
        let now = clock.frame_time();
        let since = start.get().unwrap_or(now);
        start.set(Some(since));
        let t = ((now - since) as f64 / 220_000.0).min(1.0);
        cover.set_opacity(1.0 - ease(t));
        cover.set_margin_top((ease(t) * 18.0) as i32);
        if t >= 1.0 {
            if let Some(host) = weak.upgrade() {
                host.remove_overlay(cover);
            }
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}
