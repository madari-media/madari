import Foundation

/// The controls the shared player sheet drives.
///
/// Both backends present the same audio, subtitle, volume and speed options, so the
/// sheet is written against this instead of against one concrete engine. The member
/// names match `PlayerEngine`'s, which already implemented all of them.
/// The sheet drives this from the UI and `PlayerEngine` is a main-actor type, so the
/// protocol is main-actor isolated to match. The libmpv engine's members are not
/// isolated, which is allowed: they can be called from any thread, including the UI.
@MainActor
protocol PlayerOptionsModel: ObservableObject {
    var audioTracks: [TrackOption] { get }
    var subtitleTracks: [TrackOption] { get }
    var selectedAudio: Int? { get }
    var selectedSubtitle: Int? { get }
    var volume: Double { get }
    var muted: Bool { get }

    func select(audio: Int?)
    func select(subtitle: Int?)
    func setSpeed(_ speed: Double)
    func setVolume(_ value: Double)
    func toggleMute()
}

extension PlayerOptionsModel {
    /// Whether subtitles the source declares can be attached at playback time.
    ///
    /// Only the libmpv backend can: AVFoundation offers no public API for adding a
    /// subtitle file to an item it did not demux, which is why the sheet shows these
    /// rows for one backend and not the other rather than pretending they work.
    var canAddSubtitles: Bool { false }

    func addSubtitle(url: String) {}
}

extension PlayerEngine: PlayerOptionsModel {}

extension MpvEngine: PlayerOptionsModel {
    /// libmpv can attach a subtitle file to a file that is already open, which is why
    /// only this backend offers the source's declared subtitles.
    var canAddSubtitles: Bool { true }
}
