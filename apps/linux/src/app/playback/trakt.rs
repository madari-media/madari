//! Ordered, asynchronous playback reporting; never blocks the decoder or navigation.
use crate::app::*;
use madari_native::trakt::{Action, Media, Outcome, Scrobble};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

type Reply = Result<Option<Outcome>>;
type Request = (
    madari_native::profiles::PlaybackGrant,
    Scrobble,
    oneshot::Sender<Reply>,
);
#[derive(Clone)]
pub(in crate::app) struct Queue(mpsc::Sender<Message>);
enum Message {
    Report(Request),
    Flush(oneshot::Sender<()>),
}
impl Queue {
    pub(in crate::app) fn new(runtime: &Runtime, profiles: Profiles) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Message>(64);
        runtime.spawn(async move {
            while let Some(message) = receiver.recv().await {
                match message {
                    Message::Report((session, event, reply)) => {
                        let _ = reply.send(profiles.scrobble_trakt(session, event).await);
                    }
                    Message::Flush(reply) => {
                        let _ = reply.send(());
                    }
                }
            }
        });
        Self(sender)
    }
    pub(in crate::app) async fn flush(&self) {
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            let (reply, done) = oneshot::channel();
            if self.0.send(Message::Flush(reply)).await.is_ok() {
                let _ = done.await;
            }
        })
        .await;
    }
}
pub struct Session {
    ui: std::rc::Weak<Ui>,
    queue: Queue,
    grant: RefCell<Option<madari_native::profiles::PlaybackGrant>>,
    media: Option<Media>,
    label: gtk::Label,
    ready: Cell<bool>,
    connected: Cell<bool>,
    pending: Cell<bool>,
    watched: Cell<bool>,
    ended: Cell<bool>,
    failed: Cell<bool>,
    sequence: Cell<u64>,
    last: Cell<Option<Action>>,
    attempted: Cell<Instant>,
}
fn action(position: u64, duration: u64, paused: bool, ending: bool) -> Option<(Action, f64)> {
    if duration == 0 {
        return None;
    }
    let percent = (position as f64 / duration as f64 * 100.0).clamp(0.0, 100.0);
    Some((
        if percent >= 95.0 {
            Action::Stop
        } else if paused || ending {
            Action::Pause
        } else {
            Action::Start
        },
        percent,
    ))
}
impl Session {
    pub(in crate::app) fn new(
        ui: &Rc<Ui>,
        key: &madari_model::ItemKey,
        video: &str,
        episodes: &[madari_model::Video],
        label: gtk::Label,
    ) -> Option<Rc<Self>> {
        let profile = ui.session.borrow().clone()?;
        let session = Rc::new(Self {
            ui: Rc::downgrade(ui),
            queue: ui.trakt_queue.clone(),
            grant: RefCell::new(None),
            media: Media::from_video(key, video, episodes),
            label,
            ready: Cell::new(false),
            connected: Cell::new(false),
            pending: Cell::new(false),
            watched: Cell::new(false),
            ended: Cell::new(false),
            failed: Cell::new(false),
            sequence: Cell::new(0),
            last: Cell::new(None),
            attempted: Cell::new(Instant::now()),
        });
        let profiles = ui.profiles.clone();
        let task = ui
            .runtime
            .spawn(async move { profiles.trakt_playback(profile).await });
        let weak = Rc::downgrade(&session);
        glib::spawn_future_local(async move {
            let result = task.await;
            let Some(s) = weak.upgrade() else {
                return;
            };
            s.ready.set(true);
            match result {
                Ok(Ok(Some(grant))) => {
                    *s.grant.borrow_mut() = Some(grant);
                    s.connected.set(true);
                    s.status(
                        if s.media.is_some() {
                            "Trakt · Ready"
                        } else {
                            "Trakt · Unmatched"
                        },
                        "Playback can only sync when the movie or episode has a supported ID.",
                        false,
                    );
                }
                Ok(Ok(None)) => s.label.set_visible(false),
                _ => s.status(
                    "Trakt · Unavailable",
                    "Could not check this profile’s Trakt connection.",
                    true,
                ),
            }
        });
        Some(session)
    }
    fn status(&self, text: &str, tooltip: &str, error: bool) {
        self.label.set_visible(true);
        self.label.set_text(text);
        self.label.set_tooltip_text(Some(tooltip));
        if error {
            self.label.add_css_class("error");
            self.label.remove_css_class("success");
        } else {
            self.label.remove_css_class("error");
            if text.contains("synced") || text.ends_with("Watching") {
                self.label.add_css_class("success");
            } else {
                self.label.remove_css_class("success");
            }
        }
    }
    pub fn observe(
        self: &Rc<Self>,
        position: Option<u64>,
        duration: Option<u64>,
        paused: bool,
        loaded: bool,
    ) {
        if !loaded
            || !self.ready.get()
            || !self.connected.get()
            || self.ended.get()
            || self.pending.get()
            || self.watched.get()
        {
            return;
        }
        if position == Some(0) && self.last.get().is_none() {
            return;
        }
        let Some((action, percent)) = position
            .zip(duration)
            .and_then(|(p, d)| action(p, d, paused, false))
        else {
            return;
        };
        if self.last.get() == Some(action) && !self.failed.get() {
            return;
        }
        if self.failed.get() && self.attempted.get().elapsed() < Duration::from_secs(30) {
            return;
        }
        self.send(action, percent);
    }
    pub fn finish(self: &Rc<Self>, position: Option<u64>, duration: Option<u64>, loaded: bool) {
        if self.ended.replace(true) || self.watched.get() || !loaded {
            return;
        }
        // Also queue the final event behind an in-flight start/pause. The shared
        // FIFO keeps the old episode's stop ahead of the next episode's start.
        if !self.connected.get() {
            return;
        }
        if let Some((action, percent)) = position
            .zip(duration)
            .and_then(|(p, d)| action(p, d, false, true))
        {
            if action == Action::Stop && self.last.get() == Some(Action::Stop) && self.pending.get()
            {
                return;
            }
            self.send(action, percent);
        }
    }
    fn send(self: &Rc<Self>, action: Action, progress: f64) {
        let Some(media) = self.media.clone() else {
            return;
        };
        let Some(grant) = self.grant.borrow().clone() else {
            return;
        };
        let (reply, response) = oneshot::channel();
        if self
            .queue
            .0
            .try_send(Message::Report((
                grant,
                Scrobble {
                    media,
                    action,
                    progress,
                },
                reply,
            )))
            .is_err()
        {
            self.failed.set(true);
            self.attempted.set(Instant::now());
            self.status(
                "Trakt · Not synced",
                "The Trakt queue is busy. Playback continues; reporting will retry.",
                true,
            );
            return;
        }
        self.last.set(Some(action));
        self.pending.set(true);
        self.attempted.set(Instant::now());
        let sequence = self.sequence.get().wrapping_add(1);
        self.sequence.set(sequence);
        self.status(
            "Trakt · Syncing…",
            "Sending this playback update to Trakt.",
            false,
        );
        let session = self.clone();
        glib::spawn_future_local(async move {
            let result = response.await;
            if session.sequence.get() != sequence {
                return;
            }
            session.pending.set(false);
            match result {
                Ok(Ok(Some(outcome))) => {
                    session.failed.set(false);
                    let (text, detail) = match outcome {
                        Outcome::Watching => (
                            "Trakt · Watching",
                            "Trakt confirmed that you’re watching this video.",
                        ),
                        Outcome::Paused => (
                            "Trakt · Paused synced",
                            "Trakt saved your playback progress.",
                        ),
                        Outcome::Watched => {
                            session.watched.set(true);
                            (
                                "Trakt · Watched synced",
                                "Trakt confirmed this video in your watched history.",
                            )
                        }
                    };
                    session.status(text, detail, false);
                }
                Ok(Ok(None)) => {
                    session.connected.set(false);
                    session.label.set_visible(false);
                }
                Ok(Err(e)) => {
                    session.failed.set(true);
                    session.status("Trakt · Not synced", &e.message, true);
                    if session.ended.get()
                        && let Some(ui) = session.ui.upgrade()
                    {
                        ui.toast.add_toast(adw::Toast::new("Trakt couldn’t save the last playback update. Check your connection or reconnect Trakt."));
                    }
                }
                Err(_) => {
                    session.failed.set(true);
                    session.status(
                        "Trakt · Not synced",
                        "Trakt reporting stopped before confirming the update.",
                        true,
                    );
                }
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_pause_before_95_percent_and_never_guesses_unknown_duration() {
        assert!(action(10, 0, false, false).is_none());
        assert_eq!(action(10, 100, false, false), Some((Action::Start, 10.0)));
        assert_eq!(action(90, 100, false, true), Some((Action::Pause, 90.0)));
        assert_eq!(action(50, 100, true, false), Some((Action::Pause, 50.0)));
        assert_eq!(action(95, 100, false, false), Some((Action::Stop, 95.0)));
        assert_eq!(action(200, 100, false, true), Some((Action::Stop, 100.0)));
    }
}
