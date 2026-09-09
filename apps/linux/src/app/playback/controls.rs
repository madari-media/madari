//! GTK player chrome. All actions are sent to the engine, never executed in draw callbacks.
use crate::app::playback::engine::{Chapter, Track};
use adw::prelude::*;
use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Pause,
    Mute,
    Seek(f64),
    Volume(f64),
    Speed(f64),
    Fullscreen,
    Escape,
    Pip,
    Mini,
    Stop,
    Audio,
    Sources,
    ChooseSource(usize),
    Episodes,
    PreviousEpisode,
    NextEpisode,
    Subtitles,
    Fit,
    Help,
    OpenSubtitle,
    Set(String, String),
    Run(Vec<String>),
}
type Handler = Box<dyn Fn(Action)>;
pub struct Controls {
    pub root: gtk::Overlay,
    pub area: gtk::GLArea,
    top: gtk::Revealer,
    bottom: gtk::Revealer,
    back: gtk::Button,
    pause: gtk::Button,
    mute: gtk::Button,
    volume_reveal: gtk::Revealer,
    seek: gtk::Scale,
    time: gtk::Label,
    transport: gtk::Box,
    title: gtk::Label,
    loading: gtk::Revealer,
    loading_text: gtk::Label,
    spinner: gtk::Spinner,
    osd: gtk::Label,
    osd_reveal: gtk::Revealer,
    audio_box: gtk::Box,
    sub_box: gtk::Box,
    sources_box: gtk::Box,
    chapter_box: gtk::Box,
    chapters_button: gtk::MenuButton,
    popovers: Vec<gtk::Popover>,
    volume: gtk::Scale,
    speed: gtk::Label,
    pub trakt_status: gtk::Label,
    stats_page: gtk::Box,
    stats_playback: gtk::Label,
    stats_torrent: gtk::Label,
    stats_cache: Cell<f64>,
    audio_summary: gtk::Label,
    subtitle_summary: gtk::Label,
    fit_summary: gtk::Label,
    speed_options: RefCell<Vec<(f64, gtk::Button)>>,
    fit_options: RefCell<Vec<(String, gtk::Button)>>,
    tracks: RefCell<Option<Vec<Track>>>,
    chapters: RefCell<Option<Vec<Chapter>>>,
    fullscreen: gtk::Button,
    episodes_button: gtk::Button,
    previous_episode: gtk::Button,
    next_episode: gtk::Button,
    pip: gtk::Button,
    handler: RefCell<Option<Handler>>,
    last_input: Cell<Instant>,
    last_pointer: Cell<Option<(f64, f64)>>,
    osd_until: Cell<Instant>,
    hovering: Cell<bool>,
    pub paused: Cell<bool>,
    buffering: Cell<bool>,
    opening: Cell<bool>,
    seeking: Cell<bool>,
    pub seekable: Cell<bool>,
    duration: Cell<f64>,
    display_offset: Cell<u64>,
    cache_end: Cell<f64>,
    pub current_speed: Cell<f64>,
    pub current_volume: Cell<f64>,
    updating: Cell<bool>,
    compact: Cell<bool>,
    fit: Cell<usize>,
    dragging: Cell<bool>,
    seek_generation: Cell<u64>,
    pending_seek: Cell<Option<(f64, Instant)>>,
}
fn text(value: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(value));
    l.set_use_markup(false);
    l
}
fn icon(name: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(name);
    b.set_tooltip_text(Some(tip));
    b.add_css_class("flat");
    b.add_css_class("player-icon");
    b.update_property(&[gtk::accessible::Property::Label(tip)]);
    b
}
fn settings_page(stack: &gtk::Stack, name: &str, title: &str) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    if name != "home" {
        let back = gtk::Button::from_icon_name("go-previous-symbolic");
        back.add_css_class("flat");
        back.set_tooltip_text(Some("Back to settings"));
        let weak = stack.downgrade();
        back.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                s.set_visible_child_name("home");
            }
        });
        header.append(&back);
    }
    let title = text(title);
    title.add_css_class("heading");
    title.set_wrap(true);
    title.set_margin_start(8);
    title.set_margin_top(10);
    title.set_margin_bottom(10);
    header.append(&title);
    page.append(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(390)
        .propagate_natural_height(true)
        .child(&content)
        .build();
    page.append(&scroll);
    stack.add_named(&page, Some(name));
    content
}
fn settings_link(
    stack: &gtk::Stack,
    list: &gtk::ListBox,
    page: &str,
    title: &str,
    icon_name: &str,
    summary: &str,
) -> gtk::Label {
    let row = adw::ActionRow::new();
    row.set_use_markup(false);
    row.set_title(title);
    row.set_title_lines(2);
    row.set_activatable(true);
    row.add_prefix(&gtk::Image::from_icon_name(icon_name));
    let value = text(summary);
    value.add_css_class("dim-label");
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    value.set_max_width_chars(14);
    row.add_suffix(&value);
    row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    let weak = stack.downgrade();
    let page = page.to_string();
    row.connect_activated(move |_| {
        if let Some(stack) = weak.upgrade() {
            stack.set_visible_child_name(&page);
        }
    });
    list.append(&row);
    value
}
fn inline_volume(container: &gtk::Box, reveal: &gtk::Revealer) {
    let hovered = Rc::new(Cell::new(false));
    let focused = Rc::new(Cell::new(false));
    let motion = gtk::EventControllerMotion::new();
    let inside = hovered.clone();
    let weak = reveal.downgrade();
    motion.connect_enter(move |_, _, _| {
        inside.set(true);
        if let Some(reveal) = weak.upgrade() {
            reveal.set_reveal_child(true);
        }
    });
    let inside = hovered.clone();
    let focus = focused.clone();
    let weak = reveal.downgrade();
    motion.connect_leave(move |_| {
        inside.set(false);
        let inside = inside.clone();
        let focus = focus.clone();
        let weak = weak.clone();
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            if !inside.get()
                && !focus.get()
                && let Some(reveal) = weak.upgrade()
            {
                reveal.set_reveal_child(false);
            }
        });
    });
    container.add_controller(motion);
    let focus = gtk::EventControllerFocus::new();
    let inside = focused.clone();
    let weak = reveal.downgrade();
    focus.connect_enter(move |_| {
        inside.set(true);
        if let Some(reveal) = weak.upgrade() {
            reveal.set_reveal_child(true);
        }
    });
    let weak = reveal.downgrade();
    focus.connect_leave(move |_| {
        focused.set(false);
        if !hovered.get()
            && let Some(reveal) = weak.upgrade()
        {
            reveal.set_reveal_child(false);
        }
    });
    container.add_controller(focus);
}

fn constrain_popover(popover: &gtk::Popover, root: &gtk::Overlay) {
    popover.set_position(gtk::PositionType::Top);
    popover.set_halign(gtk::Align::End);
    popover.set_offset(-8, -8);
    let root = root.downgrade();
    popover.connect_show(move |popover| {
        let Some(root) = root.upgrade() else {
            return;
        };
        let width = (root.width() - 48).clamp(160, 300);
        let height = (root.height() - 180).clamp(80, 320);
        if let Some(child) = popover.child() {
            child.set_size_request(width, -1);
            constrain_scrolls(&child, width, height);
        }
    });
}
fn constrain_scrolls(widget: &gtk::Widget, width: i32, height: i32) {
    if let Some(scroll) = widget.downcast_ref::<gtk::ScrolledWindow>() {
        scroll.set_max_content_height(height);
        scroll.set_min_content_width(width);
        scroll.set_max_content_width(width);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        constrain_scrolls(&current, width, height);
        child = current.next_sibling();
    }
}
impl Controls {
    pub fn new() -> Rc<Self> {
        let root = gtk::Overlay::new();
        root.add_css_class("video-player");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_focusable(true);
        let area = gtk::GLArea::builder()
            .hexpand(true)
            .vexpand(true)
            .auto_render(false)
            .build();
        area.set_required_version(3, 3);
        area.set_allowed_apis(gtk::gdk::GLAPI::GL);
        root.set_child(Some(&area));
        let top = gtk::Revealer::builder()
            .reveal_child(true)
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(180)
            .valign(gtk::Align::Start)
            .build();
        let top_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top_box.add_css_class("player-top");
        let back = icon("go-previous-symbolic", "Stop and return");
        let title = text("Now playing");
        title.add_css_class("player-title");
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_hexpand(true);
        title.set_xalign(0.0);
        title.set_width_chars(8);
        title.set_max_width_chars(60);
        top_box.append(&back);
        top_box.append(&title);
        top.set_child(Some(&top_box));
        root.add_overlay(&top);
        let bottom = gtk::Revealer::builder()
            .reveal_child(true)
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(180)
            .valign(gtk::Align::End)
            .build();
        let bottom_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bottom_box.add_css_class("player-bottom");
        let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.1);
        seek.set_draw_value(false);
        seek.set_sensitive(false);
        seek.set_hexpand(true);
        seek.add_css_class("player-seek");
        seek.update_property(&[gtk::accessible::Property::Label("Playback position")]);
        seek.set_show_fill_level(true);
        seek.set_restrict_to_fill_level(false);
        seek.set_fill_level(0.0);
        bottom_box.append(&seek);

        let control_bar = gtk::CenterBox::new();
        control_bar.add_css_class("player-control-row");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add_css_class("player-right-controls");
        let pause = icon("media-playback-pause-symbolic", "Pause / play (Space)");
        pause.add_css_class("player-primary");
        let previous_episode = icon("media-skip-backward-symbolic", "Previous episode");
        let next_episode = icon("media-skip-forward-symbolic", "Next episode");
        previous_episode.set_visible(false);
        next_episode.set_visible(false);
        let transport = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        transport.add_css_class("player-transport");
        transport.set_halign(gtk::Align::Center);
        let rewind = icon("media-seek-backward-symbolic", "Back 10 seconds (←)");
        let forward = icon("media-seek-forward-symbolic", "Forward 10 seconds (→)");
        for button in [&previous_episode, &rewind, &pause, &forward, &next_episode] {
            transport.append(button);
        }
        control_bar.set_center_widget(Some(&transport));
        let time = text("0:00 / —");
        time.add_css_class("player-time");
        time.set_hexpand(false);
        time.set_ellipsize(gtk::pango::EllipsizeMode::End);
        time.set_width_chars(5);
        time.set_xalign(0.0);
        time.set_margin_start(8);
        let left = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        left.set_halign(gtk::Align::Start);
        let mute = icon("audio-volume-high-symbolic", "Volume · click to mute (M)");
        left.append(&time);
        control_bar.set_start_widget(Some(&left));
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        volume.set_draw_value(false);
        volume.set_width_request(80);
        volume.set_value(100.0);
        volume.update_property(&[gtk::accessible::Property::Label("Volume")]);
        let volume_reveal = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideRight)
            .transition_duration(180)
            .child(&volume)
            .build();
        volume_reveal.add_css_class("volume-slider");
        let volume_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        volume_box.add_css_class("inline-volume");
        volume_box.append(&mute);
        volume_box.append(&volume_reveal);
        inline_volume(&volume_box, &volume_reveal);
        row.append(&volume_box);

        let settings = gtk::MenuButton::builder()
            .icon_name("emblem-system-symbolic")
            .tooltip_text("Player settings")
            .build();
        settings.add_css_class("player-icon");
        settings.update_property(&[gtk::accessible::Property::Label("Player settings")]);
        let settings_pop = gtk::Popover::new();
        settings_pop.add_css_class("player-popover");
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::SlideLeftRight)
            .transition_duration(160)
            .hhomogeneous(false)
            .vhomogeneous(false)
            .build();

        let home = settings_page(&stack, "home", "Player settings");
        let audio_box = settings_page(&stack, "audio", "Audio tracks");
        let sources_box = settings_page(&stack, "sources", "Video sources");
        let sub_box = settings_page(&stack, "subtitles", "Subtitles");
        let episodes_button = icon("video-x-generic-symbolic", "Choose episode");
        episodes_button.set_visible(false);
        left.prepend(&episodes_button);
        let chapters_button = gtk::MenuButton::builder()
            .icon_name("user-bookmarks-symbolic")
            .tooltip_text("Chapters")
            .visible(false)
            .build();
        chapters_button.add_css_class("player-icon");
        chapters_button.update_property(&[gtk::accessible::Property::Label("Chapters")]);
        let chapter_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let chapter_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(320)
            .child(&chapter_box)
            .build();
        let chapter_pop = gtk::Popover::new();
        chapter_pop.add_css_class("player-popover");
        chapter_pop.set_child(Some(&chapter_scroll));
        chapters_button.set_popover(Some(&chapter_pop));
        row.append(&chapters_button);
        let stats_button = gtk::MenuButton::builder()
            .icon_name("utilities-system-monitor-symbolic")
            .tooltip_text("Playback stats")
            .valign(gtk::Align::Center)
            .build();
        stats_button.add_css_class("player-icon");
        stats_button.update_property(&[gtk::accessible::Property::Label("Playback stats")]);
        let stats_page = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let heading = text("Playback stats");
        heading.add_css_class("heading");
        heading.set_margin_bottom(8);
        stats_page.append(&heading);
        let stats_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(320)
            .child(&stats_page)
            .build();
        let stats_pop = gtk::Popover::new();
        stats_pop.add_css_class("player-popover");
        stats_pop.set_child(Some(&stats_scroll));
        stats_button.set_popover(Some(&stats_pop));
        let trakt_status = text("Trakt · Checking…");
        trakt_status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        trakt_status.set_max_width_chars(20);
        trakt_status.set_visible(false);
        trakt_status.add_css_class("caption");
        top_box.append(&trakt_status);
        top_box.append(&stats_button);
        let stats_playback = text("Buffer: —\nPlayback speed: 1×");
        let stats_torrent = text("Direct playback · torrent stats not applicable");
        for label in [&stats_playback, &stats_torrent] {
            label.set_wrap(true);
            label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_bottom(12);
            stats_page.append(label);
        }
        let speed_box = settings_page(&stack, "speed", "Playback speed");
        let video_box = settings_page(&stack, "video", "Video size");
        let aspect_box = settings_page(&stack, "aspect", "Aspect ratio");
        let subtitle_size_box = settings_page(&stack, "subtitle-size", "Subtitle size");
        let subtitle_sync_box = settings_page(&stack, "subtitle-sync", "Subtitle synchronization");
        let audio_sync_box = settings_page(&stack, "audio-sync", "Audio synchronization");
        let advanced_page = settings_page(&stack, "advanced", "Advanced settings");
        let advanced_list = gtk::ListBox::new();
        advanced_list.set_selection_mode(gtk::SelectionMode::None);
        advanced_list.add_css_class("boxed-list");
        advanced_page.append(&advanced_list);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        home.append(&list);
        let audio_summary = settings_link(
            &stack,
            &list,
            "audio",
            "Audio tracks",
            "audio-headphones-symbolic",
            "Auto",
        );
        let subtitle_summary = settings_link(
            &stack,
            &list,
            "subtitles",
            "Subtitles",
            "media-view-subtitles-symbolic",
            "Off",
        );
        settings_link(
            &stack,
            &list,
            "sources",
            "Video sources",
            "network-server-symbolic",
            "",
        );

        let speed = settings_link(
            &stack,
            &list,
            "speed",
            "Playback speed",
            "media-playback-start-symbolic",
            "1×",
        );
        let fit_summary = settings_link(
            &stack,
            &list,
            "video",
            "Video size",
            "view-fullscreen-symbolic",
            "Fit",
        );
        settings_link(
            &stack,
            &advanced_list,
            "aspect",
            "Aspect ratio",
            "video-display-symbolic",
            "Auto",
        );
        settings_link(
            &stack,
            &advanced_list,
            "subtitle-size",
            "Subtitle size",
            "format-text-larger-symbolic",
            "",
        );
        settings_link(
            &stack,
            &advanced_list,
            "subtitle-sync",
            "Subtitle timing",
            "preferences-system-time-symbolic",
            "",
        );
        settings_link(
            &stack,
            &advanced_list,
            "audio-sync",
            "Audio timing",
            "preferences-system-time-symbolic",
            "",
        );
        settings_link(
            &stack,
            &list,
            "advanced",
            "Advanced settings",
            "preferences-other-symbolic",
            "",
        );
        settings_pop.set_child(Some(&stack));
        settings.set_popover(Some(&settings_pop));
        let weak = stack.downgrade();
        settings_pop.connect_show(move |_| {
            if let Some(stack) = weak.upgrade() {
                stack.set_visible_child_name("home");
            }
        });
        row.append(&settings);
        // Mini-player toggle stays available on the control bar.
        let pip = icon("view-restore-symbolic", "Mini player (I)");
        row.append(&pip);
        let fullscreen = icon("view-fullscreen-symbolic", "Fullscreen (F)");
        row.append(&fullscreen);
        control_bar.set_end_widget(Some(&row));
        bottom_box.append(&control_bar);
        bottom.set_child(Some(&bottom_box));
        root.add_overlay(&bottom);
        let loading = gtk::Box::new(gtk::Orientation::Vertical, 14);
        loading.set_halign(gtk::Align::Center);
        loading.set_valign(gtk::Align::Center);
        loading.set_can_target(false);
        loading.add_css_class("player-loading");
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(42, 42);
        spinner.start();
        let loading_text = text("Opening video…");
        loading.append(&spinner);
        loading.append(&loading_text);
        let loading = gtk::Revealer::builder()
            .child(&loading)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .can_target(false)
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(180)
            .reveal_child(true)
            .build();
        root.add_overlay(&loading);
        let osd = text("");
        osd.add_css_class("player-osd");
        osd.set_halign(gtk::Align::Center);
        osd.set_valign(gtk::Align::Center);
        osd.set_can_target(false);
        let osd_reveal = gtk::Revealer::builder()
            .child(&osd)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .can_target(false)
            .transition_type(gtk::RevealerTransitionType::Crossfade)
            .transition_duration(160)
            .build();
        root.add_overlay(&osd_reveal);
        let c = Rc::new(Self {
            root,
            area,
            top,
            bottom,
            back: back.clone(),
            pause,
            mute,
            volume_reveal,
            seek,
            time,
            transport,
            title,
            loading,
            loading_text,
            spinner,
            osd,
            osd_reveal,
            audio_box,
            sub_box,
            sources_box,
            chapter_box,
            chapters_button,
            popovers: vec![settings_pop, chapter_pop, stats_pop.clone()],
            volume,
            speed,
            trakt_status,
            stats_page,
            stats_playback,
            stats_torrent,
            stats_cache: Cell::new(0.0),
            audio_summary,
            subtitle_summary,
            fit_summary,
            speed_options: RefCell::new(Vec::new()),
            fit_options: RefCell::new(Vec::new()),
            tracks: RefCell::new(None),
            chapters: RefCell::new(None),
            fullscreen,
            episodes_button,
            previous_episode,
            next_episode,
            pip,
            handler: RefCell::new(None),
            last_input: Cell::new(Instant::now()),
            last_pointer: Cell::new(None),
            osd_until: Cell::new(Instant::now()),
            hovering: Cell::new(false),
            paused: Cell::new(false),
            buffering: Cell::new(true),
            opening: Cell::new(true),
            seeking: Cell::new(false),
            seekable: Cell::new(false),
            duration: Cell::new(0.0),
            display_offset: Cell::new(0),
            cache_end: Cell::new(0.0),
            current_speed: Cell::new(1.0),
            current_volume: Cell::new(100.0),
            updating: Cell::new(false),
            compact: Cell::new(false),
            fit: Cell::new(0),
            dragging: Cell::new(false),
            seek_generation: Cell::new(0),
            pending_seek: Cell::new(None),
        });
        let w = Rc::downgrade(&c);
        stack.connect_visible_child_name_notify(move |stack| {
            if stack.visible_child_name().as_deref() == Some("sources")
                && let Some(c) = w.upgrade()
            {
                c.emit(Action::Sources);
            }
        });
        c.option(&home, "Mini player", Action::Mini);
        c.option(&home, "Picture in picture", Action::Pip);
        for (button, action) in [
            (&back, Action::Stop),
            (&c.pause, Action::Pause),
            (&c.mute, Action::Mute),
            (&rewind, Action::Seek(-10.0)),
            (&forward, Action::Seek(10.0)),
            (&c.fullscreen, Action::Fullscreen),
            (&c.pip, Action::Mini),
            (&c.episodes_button, Action::Episodes),
            (&c.previous_episode, Action::PreviousEpisode),
            (&c.next_episode, Action::NextEpisode),
        ] {
            let w = Rc::downgrade(&c);
            button.connect_clicked(move |_| {
                if let Some(c) = w.upgrade() {
                    c.emit(action.clone());
                }
            });
        }
        let w = Rc::downgrade(&c);
        c.seek.connect_change_value(move |scale, _, value| {
            if let Some(c) = w.upgrade()
                && c.seekable.get()
            {
                scale.set_value(value);
                let offset = c.display_offset.get() as f64 / 1000.0;
                c.time.set_text(&format!(
                    "{} / {}",
                    timestamp(value + offset),
                    timestamp(c.duration.get() + offset)
                ));
                c.reveal();
                scale.set_tooltip_text(Some(&timestamp(value)));
                if !c.dragging.get() {
                    c.queue_seek(value);
                }
            }
            glib::Propagation::Stop
        });
        let pointer = gtk::EventControllerLegacy::new();
        pointer.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&c);
        pointer.connect_event(move |_, event| {
            let Some(c) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match event.event_type() {
                gtk::gdk::EventType::ButtonPress | gtk::gdk::EventType::TouchBegin => {
                    c.dragging.set(true);
                    c.seek_generation.set(c.seek_generation.get() + 1);
                    c.reveal();
                }
                gtk::gdk::EventType::ButtonRelease | gtk::gdk::EventType::TouchEnd => {
                    let weak = Rc::downgrade(&c);
                    glib::idle_add_local_once(move || {
                        if let Some(c) = weak.upgrade()
                            && c.dragging.replace(false)
                        {
                            c.commit_seek(c.seek.value());
                        }
                    });
                }
                gtk::gdk::EventType::TouchCancel => c.dragging.set(false),
                _ => (),
            }
            glib::Propagation::Proceed
        });
        c.seek.add_controller(pointer);
        let w = Rc::downgrade(&c);
        c.volume.connect_value_changed(move |v| {
            if let Some(c) = w.upgrade()
                && !c.updating.get()
            {
                c.emit(Action::Set("volume".into(), v.value().to_string()));
            }
        });
        let motion = gtk::EventControllerMotion::new();
        let w = Rc::downgrade(&c);
        motion.connect_motion(move |_, x, y| {
            if let Some(c) = w.upgrade()
                && c.last_pointer
                    .replace(Some((x, y)))
                    .is_none_or(|(old_x, old_y)| (x - old_x).abs() > 0.5 || (y - old_y).abs() > 0.5)
            {
                c.reveal();
            }
        });
        c.root.add_controller(motion);
        for panel in [&top_box, &bottom_box] {
            let motion = gtk::EventControllerMotion::new();
            let w = Rc::downgrade(&c);
            motion.connect_enter(move |_, _, _| {
                if let Some(c) = w.upgrade() {
                    c.hovering.set(true);
                }
            });
            let w = Rc::downgrade(&c);
            motion.connect_leave(move |_| {
                if let Some(c) = w.upgrade() {
                    c.hovering.set(false);
                    c.last_input.set(Instant::now());
                }
            });
            panel.add_controller(motion);
        }
        let keys = gtk::EventControllerKey::new();
        let w = Rc::downgrade(&c);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(c) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !shortcuts_focused(&c.root) || c.popovers.iter().any(|p| p.is_visible()) {
                return glib::Propagation::Proceed;
            }
            if mods.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            let name = key.name().unwrap_or_default();
            if let Some(action) = shortcut(&name, mods.contains(gtk::gdk::ModifierType::SHIFT_MASK))
            {
                c.emit(action);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        c.root.add_controller(keys);
        let click = gtk::GestureClick::new();
        let w = Rc::downgrade(&c);
        click.connect_released(move |_, count, _, _| {
            if let Some(c) = w.upgrade() {
                c.reveal();
                c.root.grab_focus();
                if count == 2 {
                    c.emit(Action::Fullscreen);
                }
            }
        });
        c.area.add_controller(click);
        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
        );
        let w = Rc::downgrade(&c);
        scroll.connect_scroll(move |_, _, dy| {
            if let Some(c) = w.upgrade() {
                c.emit(Action::Volume(-dy * 5.0));
            }
            glib::Propagation::Stop
        });
        c.area.add_controller(scroll);
        for speed in [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0] {
            let title = if speed == 1.0 {
                "✓ 1× (Normal)".to_string()
            } else {
                format!("{speed}×")
            };
            let button = c.option(
                &speed_box,
                &title,
                Action::Set("speed".into(), speed.to_string()),
            );
            c.speed_options.borrow_mut().push((speed, button));
        }
        for (name, value) in [
            ("Fit to window", "fit"),
            ("Fill / crop", "fill"),
            ("Stretch", "stretch"),
            ("Original size", "original"),
        ] {
            let button = c.option(
                &video_box,
                &format!("{}{name}", if value == "fit" { "✓ " } else { "" }),
                Action::Set("madari-fit".into(), value.into()),
            );
            c.fit_options.borrow_mut().push((value.into(), button));
        }

        for (title, value) in [
            ("Automatic", "-1"),
            ("16:9", "16:9"),
            ("4:3", "4:3"),
            ("Cinema 2.35:1", "2.35:1"),
        ] {
            c.option(
                &aspect_box,
                title,
                Action::Set("video-aspect-override".into(), value.into()),
            );
        }
        for (title, value) in [
            ("Small", "0.8"),
            ("Normal", "1.0"),
            ("Large", "1.3"),
            ("Extra large", "1.6"),
        ] {
            c.option(
                &subtitle_size_box,
                title,
                Action::Set("sub-scale".into(), value.into()),
            );
        }
        for (container, property) in [
            (&subtitle_sync_box, "sub-delay"),
            (&audio_sync_box, "audio-delay"),
        ] {
            for (title, delta) in [("Earlier by 0.1 s", "-0.1"), ("Later by 0.1 s", "0.1")] {
                c.option(
                    container,
                    title,
                    Action::Run(vec!["add".into(), property.into(), delta.into()]),
                );
            }
            c.option(
                container,
                "Reset timing",
                Action::Set(property.into(), "0".into()),
            );
        }
        for pop in &c.popovers {
            constrain_popover(pop, &c.root);
        }
        stats_pop.set_position(gtk::PositionType::Bottom);
        stats_pop.set_offset(-8, 8);
        let shortcuts = adw::ActionRow::new();
        shortcuts.set_use_markup(false);
        shortcuts.set_title("Keyboard shortcuts");
        shortcuts.set_activatable(true);
        shortcuts.add_prefix(&gtk::Image::from_icon_name("input-keyboard-symbolic"));
        let weak = Rc::downgrade(&c);
        shortcuts.connect_activated(move |_| {
            if let Some(c) = weak.upgrade() {
                c.emit(Action::Help);
            }
        });
        list.append(&shortcuts);
        c.refresh_tracks(&[]);
        c.refresh_chapters(&[]);
        c
    }
    pub fn bind(&self, f: impl Fn(Action) + 'static) {
        *self.handler.borrow_mut() = Some(Box::new(f));
    }
    pub fn place_skip_action(&self, widget: &impl IsA<gtk::Widget>) {
        widget.set_halign(gtk::Align::End);
        widget.set_valign(gtk::Align::End);
        widget.set_margin_end(16);
        widget.set_margin_start(16);
        let bottom = self.bottom.downgrade();
        widget.add_tick_callback(move |widget, _| {
            let Some(bottom) = bottom.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let margin = if bottom.reveals_child() {
                bottom.height() + 12
            } else {
                24
            };
            if widget.margin_bottom() != margin {
                widget.set_margin_bottom(margin);
            }
            glib::ControlFlow::Continue
        });
    }
    fn emit(&self, action: Action) {
        self.reveal();
        if let Some(f) = self.handler.borrow().as_ref() {
            f(action);
        }
    }
    fn option(self: &Rc<Self>, container: &gtk::Box, title: &str, action: Action) -> gtk::Button {
        let b = gtk::Button::with_label(title);
        if let Some(label) = b.child().and_downcast::<gtk::Label>() {
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(28);
            label.set_xalign(0.0);
        }
        b.set_tooltip_text(Some(title));
        b.add_css_class("flat");
        let w = Rc::downgrade(self);
        b.connect_clicked(move |_| {
            if let Some(c) = w.upgrade() {
                c.emit(action.clone());
            }
        });
        container.append(&b);
        b
    }
    pub fn message(&self, message: &str) {
        self.osd.set_text(message);
        self.osd_reveal.set_reveal_child(true);
        self.osd_until
            .set(Instant::now() + Duration::from_millis(1400));
        self.reveal();
    }
    pub fn reveal(&self) {
        self.last_input.set(Instant::now());
        self.top.set_reveal_child(true);
        self.bottom.set_reveal_child(true);
        self.root.set_cursor_from_name(None);
    }
    pub fn tick(&self) {
        if self.stats_visible() {
            self.stats_playback.set_text(&format!(
                "Status: {}\nBuffer ahead: {:.1} s\nPlayback speed: {}×",
                if self.buffering.get() {
                    "Buffering"
                } else if self.paused.get() {
                    "Paused"
                } else {
                    "Playing"
                },
                self.stats_cache.get(),
                self.current_speed.get()
            ));
        }

        let narrow = self.root.width() < 600;
        self.transport.set_spacing(if narrow { 2 } else { 8 });
        self.time.set_visible(!narrow);
        self.volume.set_width_request(if narrow {
            if self.chapters_button.is_visible() {
                28
            } else {
                56
            }
        } else {
            80
        });
        self.pip.set_visible(self.compact.get() || !narrow);
        if narrow {
            self.root.add_css_class("narrow-player");
        } else {
            self.root.remove_css_class("narrow-player");
        }

        if Instant::now() > self.osd_until.get() {
            self.osd_reveal.set_reveal_child(false);
        }
        if !self.buffering.get()
            && !self.dragging.get()
            && !self.volume_reveal.reveals_child()
            && !self.popovers.iter().any(|p| p.is_visible())
            && self.last_input.get().elapsed() > Duration::from_secs(3)
        {
            self.root.grab_focus();
            self.top.set_reveal_child(false);
            self.bottom.set_reveal_child(false);
            self.root.set_cursor_from_name(Some("none"));
        }
    }
    pub fn set_episodes_available(&self, available: bool) {
        self.episodes_button.set_visible(available);
    }
    pub fn episode_neighbors(
        &self,
        previous: Option<&madari_model::Video>,
        next: Option<&madari_model::Video>,
    ) {
        for (button, episode, direction) in [
            (&self.previous_episode, previous, "Previous"),
            (&self.next_episode, next, "Next"),
        ] {
            button.set_visible(episode.is_some());
            if let Some(episode) = episode {
                button.set_tooltip_text(Some(&format!(
                    "{direction}: {}",
                    crate::app::browsing::episodes::episode_title(episode)
                )));
            }
        }
    }
    pub fn set_title(&self, title: &str) {
        self.title.set_text(title);
    }
    pub fn loaded(&self) {
        self.opening.set(false);
        self.buffering.set(false);
        self.update_loading();
    }
    fn update_loading(&self) {
        let show = self.opening.get() || self.buffering.get() || self.seeking.get();
        self.loading.set_reveal_child(show);
        self.spinner.set_spinning(show);
        if show {
            self.osd_reveal.set_reveal_child(false);
        }
    }
    pub fn flag(&self, name: &str, value: bool) {
        match name {
            "pause" => {
                self.paused.set(value);
                self.pause.set_icon_name(if value {
                    "media-playback-start-symbolic"
                } else {
                    "media-playback-pause-symbolic"
                });
                if value {
                    self.reveal();
                }
            }
            "paused-for-cache" => {
                self.buffering.set(value);
                self.loading_text.set_text("Buffering…");
                self.update_loading();
            }
            "seeking" => {
                self.seeking.set(value);
                self.loading_text.set_text("Seeking…");
                self.update_loading();
            }
            "seekable" => {
                self.seekable.set(value);
                self.seek.set_sensitive(value && self.duration.get() > 0.0);
            }
            "mute" => self.mute.set_icon_name(if value {
                "audio-volume-muted-symbolic"
            } else {
                "audio-volume-high-symbolic"
            }),
            _ => (),
        }
    }
    pub fn reporting_paused(&self) -> bool {
        self.paused.get() || self.buffering.get()
    }
    pub fn stats_visible(&self) -> bool {
        self.stats_page.is_mapped()
    }
    pub fn torrent_stats(&self, stats: Option<&madari_native::internal_media::TorrentStats>) {
        self.stats_torrent.set_text(&match stats {
            Some(s) => format!("Built-in torrent · {}\nDownload: {:.2} MiB/s\nUpload: {:.2} MiB/s\nConnected peers: {}\nVideo downloaded: {:.1} / {:.1} MiB ({:.1}%)\nSpeeds cover the whole torrent.", s.state, s.download_bytes_per_second as f64 / 1048576.0, s.upload_bytes_per_second as f64 / 1048576.0, s.peers, s.downloaded as f64 / 1048576.0, s.total as f64 / 1048576.0, if s.total > 0 { (s.downloaded as f64 / s.total as f64 * 100.0).clamp(0.0, 100.0) } else { 0.0 }),
            None => "Built-in torrent · statistics unavailable".into(),
        });
    }
    pub fn stats_delivery(&self, torrent: bool, internal: bool) {
        self.stats_torrent.set_text(if !torrent {
            "Direct playback · torrent stats not applicable"
        } else if internal {
            "Built-in torrent · waiting for statistics…"
        } else {
            "External server · torrent stats unavailable"
        });
    }
    pub fn number(&self, name: &str, value: f64) {
        if name == "demuxer-cache-duration" {
            self.stats_cache.set(value.max(0.0));
        }

        match name {
            "duration" => {
                self.duration.set(value.max(0.0));
                self.seek.set_range(0.0, value.max(1.0));
                self.seek.set_sensitive(self.seekable.get() && value > 0.0);
            }
            "demuxer-cache-time" => self.cache_end.set(value),
            "cache-buffering-state" if self.buffering.get() => self
                .loading_text
                .set_text(&format!("Buffering… {:.0}%", value.clamp(0.0, 100.0))),
            "speed" => {
                self.current_speed.set(value);
                self.speed.set_text(&format!("{value}×"));
                for (speed, button) in self.speed_options.borrow().iter() {
                    button.set_label(&format!(
                        "{}{speed}×{}",
                        if (*speed - value).abs() < 0.001 {
                            "✓ "
                        } else {
                            ""
                        },
                        if *speed == 1.0 { " (Normal)" } else { "" }
                    ));
                }
            }
            "volume" => {
                self.current_volume.set(value);
                self.updating.set(true);
                self.volume.set_value(value);
                self.updating.set(false);
            }
            _ => (),
        }
    }
    fn queue_seek(self: &Rc<Self>, value: f64) {
        self.pending_seek.set(Some((value, Instant::now())));
        let generation = self.seek_generation.get() + 1;
        self.seek_generation.set(generation);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(120), move || {
            if let Some(c) = weak.upgrade()
                && c.seek_generation.get() == generation
                && !c.dragging.get()
            {
                c.commit_seek(value);
            }
        });
    }
    fn commit_seek(&self, value: f64) {
        if !self.seekable.get() {
            return;
        }
        self.pending_seek.set(Some((value, Instant::now())));
        self.emit(Action::Run(vec![
            "seek".into(),
            value.to_string(),
            "absolute+exact".into(),
        ]));
    }
    pub fn position(&self, seconds: f64, offset: u64) {
        self.display_offset.set(offset);
        if self.dragging.get() {
            return;
        }
        if let Some((target, since)) = self.pending_seek.get() {
            if (target - seconds).abs() > 0.75 && since.elapsed() < Duration::from_secs(4) {
                return;
            }
            self.pending_seek.set(None);
        }
        self.seek.set_value(seconds);
        self.time.set_text(&format!(
            "{} / {}",
            timestamp(seconds + offset as f64 / 1000.0),
            if self.duration.get() > 0.0 {
                timestamp(self.duration.get() + offset as f64 / 1000.0)
            } else {
                "Live".into()
            }
        ));
        self.seek.set_fill_level(
            self.cache_end
                .get()
                .clamp(0.0, self.duration.get().max(0.0)),
        );
    }
    pub fn set_compact(&self, compact: bool) {
        self.back.set_icon_name(if compact {
            "window-close-symbolic"
        } else {
            "go-previous-symbolic"
        });
        self.back
            .set_tooltip_text(Some(if compact { "Close video" } else { "Back" }));
        self.compact.set(compact);
        self.hovering.set(false);
        self.pip.set_visible(true);
        self.fullscreen.set_visible(!compact);
        self.pip.set_tooltip_text(Some(if compact {
            "Return to Madari (I)"
        } else {
            "Mini player (I)"
        }));
        if compact {
            self.root.add_css_class("compact-player");
        } else {
            self.root.remove_css_class("compact-player");
        }
        self.reveal();
    }
    pub fn menus_open(&self) -> bool {
        self.popovers.iter().any(|p| p.is_mapped())
    }
    pub fn close_menus(&self) {
        for pop in &self.popovers {
            pop.popdown();
        }
    }
    pub fn source_status(self: &Rc<Self>, message: &str, retry: bool) {
        while let Some(child) = self.sources_box.first_child() {
            self.sources_box.remove(&child);
        }
        self.sources_box.append(&text(message));
        if retry {
            self.option(&self.sources_box, "Retry", Action::Sources);
        }
    }
    pub fn source_choices(
        self: &Rc<Self>,
        choices: &[(String, String, madari_model::Stream, bool)],
    ) {
        while let Some(child) = self.sources_box.first_child() {
            self.sources_box.remove(&child);
        }
        let mut groups = Vec::new();
        for (index, (provider, name, stream, selected)) in choices.iter().enumerate() {
            let button = self.option(
                &self.sources_box,
                "Play source",
                Action::ChooseSource(index),
            );
            button.add_css_class("source-card");
            button.set_child(Some(&crate::app::browsing::stream_list::content(
                stream, *selected,
            )));
            button.set_sensitive(!selected);
            groups.push((provider.clone(), name.clone(), 1, button.upcast()));
        }
        if !groups.is_empty() {
            self.sources_box
                .prepend(&crate::app::browsing::stream_list::addon_filter(groups));
        }
        if choices.is_empty() {
            self.source_status("No video sources available.", true);
        }
    }
    pub fn refresh_tracks(self: &Rc<Self>, tracks: &[Track]) {
        if self.tracks.borrow().as_deref() == Some(tracks) {
            return;
        }
        *self.tracks.borrow_mut() = Some(tracks.to_vec());
        self.audio_summary.set_text(
            tracks
                .iter()
                .find(|t| t.kind == "audio" && t.selected)
                .map_or("Off", |t| t.title.as_str()),
        );
        self.subtitle_summary.set_text(
            tracks
                .iter()
                .find(|t| t.kind == "sub" && t.selected)
                .map_or("Off", |t| t.title.as_str()),
        );
        for (kind, container, property) in [
            ("audio", &self.audio_box, "aid"),
            ("sub", &self.sub_box, "sid"),
        ] {
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            container.append(&text(if kind == "audio" {
                "Audio"
            } else {
                "Subtitles"
            }));
            let active = tracks.iter().any(|t| t.kind == kind && t.selected);
            if kind == "sub" {
                self.option(
                    container,
                    if !active { "✓ Off" } else { "Off" },
                    Action::Set(property.into(), "no".into()),
                );
            }
            let matches: Vec<_> = tracks.iter().filter(|t| t.kind == kind).collect();
            if matches.is_empty() {
                container.append(&text("No tracks available"));
            }
            for track in matches {
                self.option(
                    container,
                    &format!("{}{}", if track.selected { "✓ " } else { "" }, track.title),
                    Action::Set(property.into(), track.id.to_string()),
                );
            }
            if kind == "sub" {
                container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                self.option(container, "Load subtitle file…", Action::OpenSubtitle);
            }
        }
    }
    pub fn refresh_chapters(self: &Rc<Self>, chapters: &[Chapter]) {
        if self.chapters.borrow().as_deref() == Some(chapters) {
            return;
        }
        *self.chapters.borrow_mut() = Some(chapters.to_vec());
        if chapters.is_empty() {
            self.chapters_button.popdown();
        }
        self.chapters_button.set_visible(!chapters.is_empty());
        while let Some(child) = self.chapter_box.first_child() {
            self.chapter_box.remove(&child);
        }
        self.chapter_box.append(&text("Chapters"));
        for chapter in chapters {
            self.option(
                &self.chapter_box,
                &chapter.title,
                Action::Set("chapter".into(), chapter.index.to_string()),
            );
        }
    }
    pub fn set_fit(&self, value: &str) {
        let names = [
            ("fit", "Fit to window"),
            ("fill", "Fill / crop"),
            ("stretch", "Stretch"),
            ("original", "Original size"),
        ];
        if let Some(index) = names.iter().position(|(v, _)| *v == value) {
            self.fit.set(index);
            self.fit_summary.set_text(names[index].1);
        }
        for (option, button) in self.fit_options.borrow().iter() {
            let title = names.iter().find(|(v, _)| *v == option).unwrap().1;
            button.set_label(&format!(
                "{}{title}",
                if option == value { "✓ " } else { "" }
            ));
        }
    }
    pub fn next_fit(&self) -> &'static str {
        let next = (self.fit.get() + 1) % 4;
        self.fit.set(next);
        ["fit", "fill", "stretch", "original"][next]
    }
}
pub fn timestamp(value: f64) -> String {
    let seconds = if value.is_finite() {
        value.max(0.0) as u64
    } else {
        0
    };
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}
/// Playback keys belong to the player, never to text inputs or browsing pages.
pub(in crate::app) fn shortcuts_focused(player: &gtk::Overlay) -> bool {
    let Some(mut focus) = player.root().and_then(|root| root.focus()) else {
        return false;
    };
    loop {
        if focus.is::<gtk::Editable>() || focus.is::<gtk::TextView>() {
            return false;
        }
        if focus == *player.upcast_ref::<gtk::Widget>() {
            return true;
        }
        let Some(parent) = focus.parent() else {
            return false;
        };
        focus = parent;
    }
}

pub fn shortcut(key: &str, shift: bool) -> Option<Action> {
    Some(match key {
        "space" | "k" | "K" => Action::Pause,
        "m" | "M" => Action::Mute,
        "f" | "F" => Action::Fullscreen,
        "Escape" => Action::Escape,
        "i" | "I" => Action::Mini,
        "p" | "P" => Action::Pip,
        "Left" | "j" | "J" => Action::Seek(if shift { -60.0 } else { -10.0 }),
        "Right" | "l" | "L" => Action::Seek(if shift { 60.0 } else { 10.0 }),
        "Up" => Action::Volume(5.0),
        "Down" => Action::Volume(-5.0),
        "bracketleft" => Action::Speed(-0.25),
        "bracketright" => Action::Speed(0.25),
        "c" | "C" => Action::Subtitles,
        "a" | "A" => Action::Audio,
        "b" | "B" => Action::Fit,
        "question" => Action::Help,
        "period" => Action::Run(vec!["frame-step".into()]),
        "comma" => Action::Run(vec!["frame-back-step".into()]),
        _ => return None,
    })
}
pub const SHORTCUTS: &str = "Space / K    Play or pause\n← / → / J / L    Seek 10 seconds\nShift + ← / →    Seek 1 minute\n↑ / ↓    Volume\nM    Mute\nF    Fullscreen\nEsc    Exit fullscreen / return from PiP\nI    In-app mini player\nP    Separate picture-in-picture window\nA / C    Cycle audio / subtitles\n[ / ]    Playback speed\nB    Video size mode\n, / .    Previous / next frame\n?    This help";
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GTK display"]
    fn playback_shortcuts_follow_focus() {
        gtk::init().unwrap();
        let window = gtk::Window::new();
        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let search = gtk::SearchEntry::new();
        let browse = gtk::Button::with_label("Browse");
        let player = gtk::Overlay::new();
        player.set_focusable(true);
        let fields = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let entry = gtk::Entry::new();
        let text = gtk::TextView::new();
        let play = gtk::Button::with_label("Play");
        fields.append(&entry);
        fields.append(&text);
        fields.append(&play);
        player.set_child(Some(&fields));
        page.append(&search);
        page.append(&browse);
        page.append(&player);
        window.set_child(Some(&page));
        window.present();
        for input in [
            search.upcast_ref::<gtk::Widget>(),
            browse.upcast_ref(),
            entry.upcast_ref(),
            text.upcast_ref(),
        ] {
            assert!(input.grab_focus());
            assert!(!shortcuts_focused(&player));
        }
        assert!(play.grab_focus());
        assert!(shortcuts_focused(&player));
        assert!(player.grab_focus());
        assert!(shortcuts_focused(&player));
        window.close();
    }

    #[test]
    fn time_and_shortcuts_handle_long_videos_and_seek_steps() {
        assert_eq!(timestamp(3665.0), "1:01:05");
        assert_eq!(timestamp(f64::NAN), "0:00");
        assert_eq!(shortcut("Left", true), Some(Action::Seek(-60.0)));
        assert_eq!(shortcut("bracketright", false), Some(Action::Speed(0.25)));
        assert_eq!(shortcut("q", false), None);
    }
}
