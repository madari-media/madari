import Foundation
import Mpv

/// Playback through libmpv.
///
/// AVFoundation decodes MP4/MOV/M4V/HLS and nothing else, so Matroska, WebM and AVI —
/// which is what most torrent releases are — need a different decoder. This engine
/// drives the libmpv that `scripts/fetch-ios-mpv.sh` vendors; see docs/ios.md.
///
/// It mirrors `PlayerEngine`'s published surface so the two backends stay swappable,
/// and it keeps torrent bytes inside the process: `madari-internal://` sources are read
/// through the core's seekable reader with `mpv_stream_cb_add_ro`, the same reader
/// `TorrentResourceLoader` hands to AVPlayer.
///
/// Threading: mpv calls the stream callbacks on its own threads and expects them to
/// block, so those calls go straight to the (synchronous) FFI rather than through the
/// async `NativeCore` wrappers. Every published value is applied on the main queue.
final class MpvEngine: ObservableObject, @unchecked Sendable {
    @Published private(set) var position: Double = 0
    @Published private(set) var duration: Double = 0
    @Published private(set) var playing = false
    @Published private(set) var buffering = false
    @Published private(set) var ended = false
    @Published private(set) var failure: String?
    @Published private(set) var audioTracks: [TrackOption] = []
    @Published private(set) var subtitleTracks: [TrackOption] = []
    @Published private(set) var selectedAudio: Int?
    @Published private(set) var selectedSubtitle: Int?
    @Published private(set) var volume: Double = 1
    @Published private(set) var muted = false

    /// The mpv handle. The rendering surface needs it to create its render context.
    private(set) var handle: OpaquePointer?

    /// Set by `MpvSurface` so the render context can be released before mpv itself,
    /// which is the order mpv requires.
    weak var surface: MpvSurfaceView?

    private let uri: String
    private let headers: [String: String]
    private let internalToken: String?
    private let startSeconds: Double

    private var eventThread: Thread?
    private var terminated = false
    /// Bounded so the trace stays readable; enough to see whether mpv is emitting at all.
    private var eventsSeen = 0

    /// mpv's `pause` is false while the core is merely idle, so it cannot be used as
    /// "playing" on its own: that made the player report playback before anything had
    /// loaded, which then hid the controls over a black screen. `core-idle` says whether
    /// there is actually something playing.
    private var paused = false
    private var coreIdle = true
    private let stateLock = NSLock()

    /// The streams handed to mpv, retained here rather than by mpv's cookie.
    ///
    /// mpv can close a stream while a read on another thread is still in flight, so
    /// letting close_fn release the cookie frees the object underneath that read. The
    /// engine owns them instead and drops them once mpv is gone, which is after every
    /// callback has stopped.
    private var streams: [MpvStream] = []
    private let streamsLock = NSLock()

    /// Set between `start()` and the moment video output exists. mpv needs a render
    /// context before it decodes the first frame, and the surface is created in an order
    /// SwiftUI decides, so the load waits for whichever half arrives second. Loading
    /// first is what left the player hanging with a file that never started.
    private var pendingLoad = false

    /// - Parameters:
    ///   - uri: `madari-internal://<token>` for torrents, otherwise the source URL.
    ///   - headers: per-source credentials, passed to mpv as `http-header-fields`.
    init(uri: String, headers: [String: String], startSeconds: Double) {
        self.uri = uri
        self.headers = headers
        self.startSeconds = startSeconds
        if let range = uri.range(of: "madari-internal://") {
            self.internalToken = String(uri[range.upperBound...])
        } else {
            self.internalToken = nil
        }
    }

    /// Identifies this engine's mpv handle in the trace, so two engines cannot be
    /// mistaken for one.
    private var tag: String {
        handle.map { "\(UInt(bitPattern: $0))" } ?? "none"
    }

    deinit {
        // A deinit that still owns a live handle would leave the decoder threads behind.
        if let handle, !terminated {
            DebugLog.write("mpv: deinit terminating handle \(tag)")
            mpv_terminate_destroy(handle)
        }
    }

    // MARK: - Lifecycle

    /// Creates and initialises mpv, then starts loading the source.
    ///
    /// Main-actor isolated: it hands the render context to a UIKit surface, which Swift 6
    /// rightly refuses to do from a nonisolated context. Every caller is a SwiftUI view.
    @MainActor
    func start() {
        guard handle == nil else { return }
        guard let mpv = mpv_create() else {
            failure = "libmpv could not be initialised."
            return
        }
        handle = mpv

        // `vo=libmpv` means the app owns presentation through the render API.
        // `auto-safe` prefers the VideoToolbox decoder but still falls back to software
        // rather than failing outright, which matters more than the last bit of speed.
        set("vo", "libmpv")
        set("gpu-api", "opengl")
        // Hardware decode. Both modes were blamed for the crash on the first frame, but the
        // crash reports showed it was the render update callback trapping on mpv's video
        // thread, independent of the decoder. `auto-safe` prefers VideoToolbox through the
        // zero-copy GLES interop and falls back if a stream cannot use it.
        set("hwdec", "auto-safe")
        set("ao", "audiounit")
        set("config", "no")
        set("terminal", "no")
        set("input-default-bindings", "no")
        set("osc", "no")
        set("audio-client-name", "Madari")
        set("keep-open", "no")
        // libmpv must stay alive with nothing to play: `idle=no` makes the core quit as
        // soon as the playlist ends, and at startup the playlist is empty, so mpv shut
        // itself down a millisecond after initialising and every later command went to a
        // dead handle. The file is loaded once video output exists, which is always after
        // startup. This was why nothing ever played, at any download percentage.
        set("idle", "yes")
        // Verbose while the playback path is still being brought up: at "warn" mpv says
        // nothing at all about a source it cannot open, which is exactly the case that
        // needs explaining. Tighten this once playback is confirmed on device.
        set("msg-level", "all=v")
        // Embedded tracks are common in Matroska, and libass renders the text ones.
        set("sub-auto", "fuzzy")
        if !headers.isEmpty {
            set("http-header-fields", headers.map { "\($0.key): \($0.value)" }.joined(separator: ","))
        }

        guard mpv_initialize(mpv) >= 0 else {
            failure = "libmpv could not be initialised."
            return
        }

        if internalToken != nil {
            registerStream()
        }
        // Start the UI from mpv's own values rather than assuming defaults.
        if let initial = doubleProperty("volume") { volume = initial / 100 }
        observe()
        startEventLoop()
        // mpv's own messages are the only way to see demuxer and cache behaviour on a
        // device with no debugger attached, so they go into the same trace file.
        let logResult = mpv_request_log_messages(mpv, "v")
        DebugLog.write("mpv: requested verbose logging -> \(logResult)")
        pendingLoad = true
        surface?.prepareRenderContext()
        if surface == nil {
            // No surface yet means no video output at all; load anyway so the audio of a
            // supported file still plays.
            pendingLoad = false
            load()
        }
        DebugLog.write("mpv: started handle \(tag), surface=\(surface == nil ? "absent" : "present")")
    }

    /// Called when the rendering surface exists, whichever order it arrives in.
    @MainActor
    func surfaceAttached() {
        guard handle != nil else { return }
        surface?.prepareRenderContext()
    }

    /// Called by the surface once its render context exists, which is what unblocks the
    /// load that `start()` deferred.
    @MainActor
    func videoOutputReady() {
        DebugLog.write("mpv: video output ready, pendingLoad=\(pendingLoad)")
        guard pendingLoad else { return }
        pendingLoad = false
        load()
    }

    /// Stops playback and tears the decoder down. The surface must release its render
    /// context first; mpv requires that ordering.
    @MainActor
    func stop() {
        guard let handle, !terminated else { return }
        DebugLog.write("mpv: stop() on handle \(tag)")
        surface?.teardownRenderContext()
        terminated = true
        command(["stop"])
        mpv_wakeup(handle)
        self.handle = nil
        mpv_terminate_destroy(handle)
        streamsLock.withLock { streams.removeAll() }
    }

    // MARK: - Transport

    func play() {
        setProperty("pause", "no")
    }

    func pause() {
        setProperty("pause", "yes")
    }

    func togglePlayPause() {
        guard let handle else { return }
        var paused: Int32 = 0
        mpv_get_property(handle, "pause", MPV_FORMAT_FLAG, &paused)
        setProperty("pause", paused == 0 ? "yes" : "no")
    }

    func seek(to seconds: Double) {
        // `absolute` rather than a relative jump, so a scrub target is exact even while
        // an earlier seek is still settling.
        command(["seek", String(seconds), "absolute"])
    }

    func setSpeed(_ speed: Double) {
        setProperty("speed", String(speed))
    }

    /// mpv's volume is 0-100, while the player's UI works in 0-1 like AVPlayer's.
    func setVolume(_ value: Double) {
        setProperty("volume", String((value * 100).rounded()))
        if value > 0 { setProperty("mute", "no") }
        publish { $0.volume = value }
    }

    func toggleMute() {
        setProperty("mute", muted ? "no" : "yes")
        publish { $0.muted.toggle() }
    }

    /// The core records progress in milliseconds, while mpv's clock is in seconds.
    /// Converting here keeps the two units apart at the call sites, which is what left
    /// Continue watching and resume stuck near the start of every title.
    var positionMs: Double { position * 1000 }
    var durationMs: Double { duration * 1000 }

    /// Maps the app's picture-size modes, the same 0/1/2 the AVPlayer surface uses for
    /// its video gravity, onto mpv's video output properties.
    func setPictureSize(_ mode: Int) {
        setProperty("video-zoom", "0")
        switch mode {
        case 1:
            // Zoom: fill the screen and crop whatever overflows.
            setProperty("panscan", "1")
            setProperty("keepaspect", "yes")
        case 2:
            // Stretch: ignore the source's own aspect ratio.
            setProperty("panscan", "0")
            setProperty("keepaspect", "no")
        default:
            // Fit: whole frame, letterboxed.
            setProperty("panscan", "0")
            setProperty("keepaspect", "yes")
        }
    }

    /// Attaches a subtitle the source addon declared. AVFoundation cannot do this, so it
    /// is only offered by this backend; see `PlayerOptionsModel`.
    func addSubtitle(url: String) {
        command(["sub-add", url, "select"])
        refreshTracks()
    }

    func select(audio id: Int?) {
        setProperty("aid", id.map(String.init) ?? "no")
        refreshTracks()
    }

    func select(subtitle id: Int?) {
        setProperty("sid", id.map(String.init) ?? "no")
        refreshTracks()
    }

    // MARK: - Tracks

    /// Reads the track list and the current selection.
    ///
    /// mpv exposes every array element as its own property path, so this needs none of
    /// the `mpv_node` union handling that the tree API would require. Track ids are
    /// sparse and are exactly what `aid` and `sid` accept.
    func refreshTracks() {
        var audio: [TrackOption] = []
        var subtitles: [TrackOption] = []
        var audioSelected: Int?
        var subtitleSelected: Int?
        for index in 0..<max(0, intProperty("track-list/count")) {
            let id = intProperty("track-list/\(index)/id")
            guard id > 0 else { continue }
            let option = TrackOption(
                id: id,
                label: stringProperty("track-list/\(index)/title"),
                language: stringProperty("track-list/\(index)/lang")
            )
            let selected = flagProperty("track-list/\(index)/selected")
            switch stringProperty("track-list/\(index)/type") {
            case "audio":
                audio.append(option)
                if selected { audioSelected = id }
            case "sub":
                subtitles.append(option)
                if selected { subtitleSelected = id }
            default:
                break
            }
        }
        // A nil selection means the track is off, which the sheet renders as such. The
        // values are bound to `let` so the main-queue closure captures values, not vars.
        let collectedAudio = audio
        let collectedSubtitles = subtitles
        let collectedAudioSelected = audioSelected
        let collectedSubtitleSelected = subtitleSelected
        publish { engine in
            engine.audioTracks = collectedAudio
            engine.subtitleTracks = collectedSubtitles
            engine.selectedAudio = collectedAudioSelected
            engine.selectedSubtitle = collectedSubtitleSelected
        }
    }

    private func intProperty(_ name: String) -> Int {
        guard let handle else { return 0 }
        var value: Int64 = 0
        guard mpv_get_property(handle, name, MPV_FORMAT_INT64, &value) >= 0 else { return 0 }
        return Int(value)
    }

    private func flagProperty(_ name: String) -> Bool {
        guard let handle else { return false }
        var value: Int32 = 0
        guard mpv_get_property(handle, name, MPV_FORMAT_FLAG, &value) >= 0 else { return false }
        return value != 0
    }

    private func doubleProperty(_ name: String) -> Double? {
        guard let handle else { return nil }
        var value = 0.0
        guard mpv_get_property(handle, name, MPV_FORMAT_DOUBLE, &value) >= 0 else { return nil }
        return value
    }

    private func stringProperty(_ name: String) -> String {
        guard let handle, let raw = mpv_get_property_string(handle, name) else { return "" }
        defer { mpv_free(raw) }
        return String(cString: raw)
    }

    /// Reported by the rendering surface when mpv cannot create a render context, which
    /// is a failure the player has to show rather than a crash.
    func reportRenderFailure(_ message: String) {
        DebugLog.write("mpv: render context failed: \(message)")
        publish { $0.failure = "Video output could not be started: \(message)." }
    }

    // MARK: - mpv plumbing

    private func set(_ name: String, _ value: String) {
        guard let handle else { return }
        mpv_set_option_string(handle, name, value)
    }

    private func setProperty(_ name: String, _ value: String) {
        guard let handle else { return }
        mpv_set_property_string(handle, name, value)
    }

    /// Runs an mpv command. The argv and its strings have to outlive the call, and mpv
    /// expects a NULL-terminated array, so both are built and released here.
    private func command(_ args: [String]) {
        guard let handle else { return }
        let owned = args.map { strdup($0) }
        defer { owned.forEach { free($0) } }
        var argv: [UnsafePointer<CChar>?] = owned.map { UnsafePointer($0) }
        argv.append(nil)
        let result = argv.withUnsafeMutableBufferPointer { buffer in
            mpv_command(handle, buffer.baseAddress)
        }
        if result < 0 {
            DebugLog.write("mpv: command \(args.first ?? "?") failed: \(String(cString: mpv_error_string(result)))")
        }
    }

    private func registerStream() {
        guard let handle else { return }
        let registered = mpv_stream_cb_add_ro(
            handle,
            "madari",
            Unmanaged.passUnretained(self).toOpaque(),
            { userData, _, info in
                guard let userData, let info else { return -1 }
                return Unmanaged<MpvEngine>.fromOpaque(userData).takeUnretainedValue().openStream(info)
            }
        )
        DebugLog.write("mpv: stream callback registration -> \(registered)")
    }

    /// Called on an mpv thread when `madari://` is loaded. The cookie is the stream
    /// object, which mpv hands to every other callback and releases through close_fn.
    private func openStream(_ info: UnsafeMutablePointer<mpv_stream_cb_info>) -> Int32 {
        guard let token = internalToken else { return -1 }
        // Logged before the core is entered, so the trace separates "mpv never asked for
        // the stream" from "the core never answered".
        DebugLog.write("mpv: opening stream \(token.prefix(8))…")
        do {
            let reader = try NativeCore.shared.openMediaSync(uri: "madari-internal://\(token)", position: 0)
            DebugLog.write("mpv: core opened reader \(reader)")
            let stream = MpvStream(handle: reader)
            streamsLock.withLock { streams.append(stream) }
            let cookie = Unmanaged.passUnretained(stream).toOpaque()
            DebugLog.write("mpv: stream opened for \(token.prefix(8))…")
            info.pointee.cookie = cookie
            info.pointee.read_fn = { cookie, buffer, count in
                guard let cookie else { return -1 }
                return Unmanaged<MpvStream>.fromOpaque(cookie).takeUnretainedValue().read(buffer, count)
            }
            info.pointee.seek_fn = { cookie, offset in
                guard let cookie else { return -1 }
                return Unmanaged<MpvStream>.fromOpaque(cookie).takeUnretainedValue().seek(offset)
            }
            info.pointee.size_fn = { cookie in
                guard let cookie else { return -1 }
                return Unmanaged<MpvStream>.fromOpaque(cookie).takeUnretainedValue().size()
            }
            info.pointee.cancel_fn = { cookie in
                guard let cookie else { return }
                Unmanaged<MpvStream>.fromOpaque(cookie).takeUnretainedValue().cancel()
            }
            info.pointee.close_fn = { cookie in
                guard let cookie else { return }
                Unmanaged<MpvStream>.fromOpaque(cookie).takeUnretainedValue().close()
            }
            return 0
        } catch {
            let message = (error as? CoreError)?.message ?? error.localizedDescription
            DebugLog.write("mpv: stream open failed: \(message)")
            publish { $0.failure = message }
            return -1
        }
    }

    private func load() {
        let target = internalToken != nil ? "madari://\(internalToken ?? "")" : uri
        DebugLog.write("mpv: loadfile \(target)")
        command(["loadfile", target])
    }

    /// Observes the properties the UI shows, so state arrives per frame instead of
    /// being polled from Swift.
    private func observe() {
        guard let handle else { return }
        mpv_observe_property(handle, 0, "time-pos", MPV_FORMAT_DOUBLE)
        mpv_observe_property(handle, 0, "duration", MPV_FORMAT_DOUBLE)
        mpv_observe_property(handle, 0, "pause", MPV_FORMAT_FLAG)
        mpv_observe_property(handle, 0, "core-idle", MPV_FORMAT_FLAG)
        mpv_observe_property(handle, 0, "paused-for-cache", MPV_FORMAT_FLAG)
        mpv_observe_property(handle, 0, "eof-reached", MPV_FORMAT_FLAG)
        mpv_observe_property(handle, 0, "volume", MPV_FORMAT_DOUBLE)
        mpv_observe_property(handle, 0, "mute", MPV_FORMAT_FLAG)
        mpv_observe_property(handle, 0, "aid", MPV_FORMAT_INT64)
        mpv_observe_property(handle, 0, "sid", MPV_FORMAT_INT64)
        // Observed without a format: this is only a notification that the list changed.
        mpv_observe_property(handle, 0, "track-list", MPV_FORMAT_NONE)
    }

    private func startEventLoop() {
        let thread = Thread { [weak self] in self?.runEventLoop() }
        thread.name = "media.madari.mpv.events"
        thread.qualityOfService = .userInitiated
        eventThread = thread
        thread.start()
        DebugLog.write("mpv: event loop started for handle \(tag)")
    }

    private func runEventLoop() {
        guard let handle else { return }
        while !terminated {
            guard let event = mpv_wait_event(handle, -1) else { break }
            // Log messages would otherwise consume the whole budget: at verbose level they
            // arrive continuously and crowd out the events that mark real progress.
            if eventsSeen < 40, event.pointee.event_id != MPV_EVENT_LOG_MESSAGE {
                eventsSeen += 1
                let raw = mpv_event_name(event.pointee.event_id)
                DebugLog.write("mpv: event \(raw == nil ? "?" : String(cString: raw!))")
            }
            switch event.pointee.event_id {
            case MPV_EVENT_NONE:
                break
            case MPV_EVENT_SHUTDOWN:
                return
            case MPV_EVENT_FILE_LOADED:
                // Which decoder actually engaged, so hardware decode can be confirmed from
                // the trace rather than assumed.
                DebugLog.write("mpv: hwdec-current=\(stringProperty("hwdec-current")) interop=\(stringProperty("hwdec-interop"))")
                DebugLog.write("mpv: file loaded, tracks will follow")
                // Resuming through a seek after the file is open is reliable, whereas
                // `loadfile`'s per-file option string is easy to get wrong.
                if startSeconds > 0 {
                    seek(to: startSeconds)
                }
                refreshTracks()
            case MPV_EVENT_PROPERTY_CHANGE:
                handleProperty(event.pointee.data)
            case MPV_EVENT_END_FILE:
                handleEndFile(event.pointee.data)
            case MPV_EVENT_LOG_MESSAGE:
                DebugLog.write(Self.logLine(event.pointee.data))
            default:
                break
            }
        }
    }

    private func handleProperty(_ raw: UnsafeMutableRawPointer?) {
        guard let raw else { return }
        let property = raw.assumingMemoryBound(to: mpv_event_property.self).pointee
        guard let name = property.name else { return }
        let key = String(cString: name)
        // The track list is observed as a notification only, so it arrives with no value.
        if key == "track-list" {
            refreshTracks()
            return
        }
        // MPV_FORMAT_NONE means the property exists but has no value yet, which is how
        // mpv reports time-pos before the first frame is decoded.
        guard property.format != MPV_FORMAT_NONE, let data = property.data else { return }
        switch property.format {
        case MPV_FORMAT_DOUBLE:
            let value = data.assumingMemoryBound(to: Double.self).pointee
            guard value.isFinite else { return }
            publish { engine in
                if key == "time-pos", value >= 0 { engine.position = value }
                if key == "duration", value > 0 { engine.duration = value }
                // mpv's volume is 0-100; the player works in 0-1 like AVPlayer.
                if key == "volume" { engine.volume = value / 100 }
            }
        case MPV_FORMAT_INT64:
            // A track selection changed, so the sheet's checkmarks have to move.
            if key == "aid" || key == "sid" { refreshTracks() }
        case MPV_FORMAT_FLAG:
            let flag = data.assumingMemoryBound(to: Int32.self).pointee != 0
            // `pause` and `core-idle` only mean something together, so recompute the
            // reported state whenever either of them changes.
            if key == "pause" || key == "core-idle" {
                let isPlaying = stateLock.withLock { () -> Bool in
                    if key == "pause" { paused = flag } else { coreIdle = flag }
                    return !paused && !coreIdle
                }
                publish { $0.playing = isPlaying }
                return
            }
            publish { engine in
                switch key {
                case "paused-for-cache": engine.buffering = flag
                case "eof-reached": if flag { engine.ended = true }
                case "mute": engine.muted = flag
                default: break
                }
            }
        default:
            break
        }
    }

    private func handleEndFile(_ raw: UnsafeMutableRawPointer?) {
        guard let raw else { return }
        let end = raw.assumingMemoryBound(to: mpv_event_end_file.self).pointee
        switch end.reason {
        case MPV_END_FILE_REASON_EOF:
            publish { $0.ended = true }
        case MPV_END_FILE_REASON_ERROR:
            let message = String(cString: mpv_error_string(end.error))
            DebugLog.write("mpv: end file error: \(message)")
            publish { $0.failure = "This file could not be played: \(message)." }
        default:
            break
        }
    }

    /// Formats one mpv log message for the trace file.
    private static func logLine(_ raw: UnsafeMutableRawPointer?) -> String {
        guard let raw else { return "mpv: (empty log event)" }
        let message = raw.assumingMemoryBound(to: mpv_event_log_message.self).pointee
        let prefix = message.prefix.map { String(cString: $0) } ?? "?"
        let level = message.level.map { String(cString: $0) } ?? "?"
        let text = message.text.map { String(cString: $0) } ?? ""
        return "mpv[\(prefix)/\(level)]: \(text.trimmingCharacters(in: .whitespacesAndNewlines))"
    }

    /// Applies a change on the main queue, while the object is still alive.
    private func publish(_ change: @escaping @Sendable (MpvEngine) -> Void) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            change(self)
        }
    }
}

/// One open `madari-internal://` stream.
///
/// mpv reads from its own threads and blocks inside these callbacks, so reads go to the
/// core synchronously. `.pending` means the torrent piece has not arrived yet; that is
/// not end of stream, so the read waits and asks again.
private final class MpvStream: @unchecked Sendable {
    private let handle: Int64
    private let lock = NSLock()
    private var cancelled = false
    private var position: Int64 = 0
    private var readCount = 0

    /// How long a read waits for torrent data before giving up. mpv blocks here while it
    /// fills its cache, so the budget is generous: reaching a piece the torrent has to
    /// fetch first is normal, and mpv is the thing that should decide when the wait has
    /// gone on too long.
    private static let stallSeconds: Double = 600

    init(handle: Int64) {
        self.handle = handle
    }

    func read(_ buffer: UnsafeMutablePointer<CChar>?, _ count: UInt64) -> Int64 {
        guard let buffer, count > 0 else { return 0 }
        // The opening reads are what tell us whether mpv is probing the head or seeking to
        // the tail for the index, which is the difference between playing and stalling on
        // a partially downloaded torrent.
        let index = lock.withLock { readCount }
        let trace = index < 12
        if trace { DebugLog.write("mpv: read #\(index) at \(position) for \(count)") }
        let deadline = Date().addingTimeInterval(Self.stallSeconds)
        // Reported once per read call, so a stalled stream is visible without flooding
        // the trace at 20 lines a second.
        var reportedPending = false
        while true {
            if isCancelled { return -1 }
            do {
                switch try NativeCore.shared.readMediaSync(handle: handle, position: position, length: Int64(count)) {
                case .pending:
                    if trace, !reportedPending, Date() > deadline.addingTimeInterval(-Self.stallSeconds + 5) {
                        reportedPending = true
                        DebugLog.write("mpv: read #\(index) still pending at \(position) after 5s")
                    }
                    if Date() > deadline {
                        DebugLog.write("mpv: stream read stalled at \(position)")
                        return -1
                    }
                    Thread.sleep(forTimeInterval: 0.05)
                case .eof:
                    if trace { DebugLog.write("mpv: read #\(index) eof at \(position)") }
                    return 0
                case .data(let bytes):
                    if bytes.isEmpty { return 0 }
                    bytes.copyBytes(to: UnsafeMutableRawBufferPointer(start: buffer, count: bytes.count))
                    if trace { DebugLog.write("mpv: read #\(index) got \(bytes.count) bytes at \(position)") }
                    lock.withLock {
                        position += Int64(bytes.count)
                        readCount += 1
                    }
                    return Int64(bytes.count)
                }
            } catch {
                DebugLog.write("mpv: read failed at \(position): \((error as? CoreError)?.message ?? "\(error)")")
                return -1
            }
        }
    }

    func seek(_ offset: Int64) -> Int64 {
        lock.withLock {
            position = max(0, offset)
            return position
        }
    }

    func size() -> Int64 {
        (try? NativeCore.shared.mediaLengthSync(handle: handle)) ?? -1
    }

    func cancel() {
        lock.withLock { cancelled = true }
    }

    func close() {
        lock.withLock { cancelled = true }
        NativeCore.shared.closeMedia(handle: handle)
    }

    private var isCancelled: Bool { lock.withLock { cancelled } }
}
