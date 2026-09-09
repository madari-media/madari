//! Seekable in-memory bridge from the torrent engine to libmpv's IO threads.
use libmpv2::{Mpv, protocol::Protocol};
use madari_native::internal_media::{InternalMedia, MediaFile};
use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    runtime::Handle,
};
use tokio_util::sync::CancellationToken;

type Source = AssertUnwindSafe<(Arc<InternalMedia>, Handle, CancellationToken)>;
type Reader = AssertUnwindSafe<(MediaFile, Handle, CancellationToken)>;

pub fn register(
    mpv: &Mpv,
    media: Arc<InternalMedia>,
    runtime: Handle,
    cancel: CancellationToken,
) -> libmpv2::Result<Protocol<'_, Reader, Source>> {
    // Callbacks only perform media IO. They never call libmpv or touch GTK.
    let protocol = unsafe {
        Protocol::new(
            mpv,
            "madari-internal".into(),
            AssertUnwindSafe((media, runtime, cancel)),
            |source: &mut Source, uri| {
                let (media, runtime, cancel) = &source.0;
                let file = runtime
                    .block_on(media.open(uri))
                    .expect("internal media open failed");
                AssertUnwindSafe((file, runtime.clone(), cancel.clone()))
            },
            drop,
            |reader: &mut Reader, output| {
                let (file, runtime, cancel) = &mut reader.0;
                // c_char and u8 have identical size and alignment; mpv owns this buffer.
                let output =
                    std::slice::from_raw_parts_mut(output.as_mut_ptr().cast::<u8>(), output.len());
                runtime
                    .block_on(async {
                        tokio::select! {
                            result = tokio::time::timeout(Duration::from_secs(60), file.reader.read(output)) => result.ok().and_then(Result::ok),
                            _ = cancel.cancelled() => None,
                        }
                    })
                    .map_or(-1, |n| n as i64)
            },
            Some(|reader: &mut Reader, offset| {
                if offset < 0 {
                    return -1;
                }
                let (file, runtime, _) = &mut reader.0;
                runtime
                    .block_on(file.reader.seek(std::io::SeekFrom::Start(offset as u64)))
                    .map_or(-1, |n| n as i64)
            }),
            Some(|reader: &mut Reader| reader.0.0.length.min(i64::MAX as u64) as i64),
        )
    };
    protocol.register()?;
    Ok(protocol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use madari_media::{TorrentEngine, TorrentInput};
    use madari_model::{Result, Torrent, TorrentFile};
    struct Fixture(Vec<u8>);
    #[async_trait::async_trait]
    impl TorrentEngine for Fixture {
        async fn add(&self, _: TorrentInput) -> Result<Torrent> {
            self.details("fixture")
        }
        fn list(&self) -> Result<Vec<Torrent>> {
            Ok(vec![self.details("fixture")?])
        }
        fn details(&self, _: &str) -> Result<Torrent> {
            Ok(Torrent {
                id: "fixture".into(),
                name: Some("Fixture".into()),
                files: vec![TorrentFile {
                    index: 0,
                    name: "movie.mkv".into(),
                    length: self.0.len() as u64,
                }],
            })
        }
        async fn open(&self, _: &str, _: usize) -> Result<MediaFile> {
            Ok(MediaFile {
                length: self.0.len() as u64,
                mime: "video/x-matroska".into(),
                reader: Box::new(std::io::Cursor::new(self.0.clone())),
            })
        }
        async fn remove(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn shutdown(&self) {}
    }
    #[test]
    fn internal_video_plays_seeks_and_revokes_without_http() {
        crate::app::playback::embedded::initialize_locale();
        let (_dir, path) = crate::app::playback::engine::video_fixture();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let media = Arc::new(InternalMedia::from_engine(Arc::new(Fixture(
            std::fs::read(path).unwrap(),
        ))));
        assert!(runtime.block_on(media.ticket("fixture", 99)).is_err());
        let ticket = runtime.block_on(media.ticket("fixture", 0)).unwrap();
        let uri = format!("madari-internal://{}", ticket.token);
        assert!(
            runtime
                .block_on(media.open("madari-internal://invalid"))
                .is_err()
        );
        let mpv = Mpv::with_initializer(|i| {
            i.set_option("vo", "null")?;
            i.set_option("ao", "null")?;
            i.set_option("pause", "yes")?;
            i.set_option("terminal", "no")?;
            Ok(())
        })
        .unwrap();
        let _protocol = register(
            &mpv,
            media.clone(),
            runtime.handle().clone(),
            CancellationToken::new(),
        )
        .unwrap();
        mpv.command("loadfile", &[&uri]).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while mpv.get_property::<f64>("duration").unwrap_or(0.0) <= 0.0 {
            assert!(
                std::time::Instant::now() < deadline,
                "internal video failed to load"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(mpv.get_property::<bool>("seekable").unwrap());
        mpv.command("seek", &["5", "absolute+exact"]).unwrap();
        while (mpv.get_property::<f64>("time-pos").unwrap_or(0.0) - 5.0).abs() > 0.3 {
            assert!(std::time::Instant::now() < deadline, "internal seek failed");
            std::thread::sleep(Duration::from_millis(20));
        }
        mpv.command("stop", &[]).unwrap();
        media.revoke(&ticket.token);
        assert!(runtime.block_on(media.open(&uri)).is_err());
    }
}
