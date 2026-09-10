import Foundation

/// A playback backend that can be asked to stand down.
///
/// The two players are separate engines — an `AVPlayer` and a libmpv handle — so nothing
/// stops both from being alive at once. Handing a source from one to the other, or
/// presenting a new one, leaves the previous engine still playing, and the two audio
/// streams run together. This gives the app a single owner of "who is playing".
@MainActor
protocol ExclusivePlayback: AnyObject {
    func standDown()
}

/// Pauses whichever player is active when another takes over.
///
/// Players claim playback when they appear and release it when they go away, so resuming
/// in one place pauses the other rather than mixing the two.
@MainActor
final class PlaybackCoordinator {
    static let shared = PlaybackCoordinator()

    private weak var active: (any ExclusivePlayback)?

    private init() {}

    /// Claims playback for `player`, pausing whoever held it before.
    func activate(_ player: any ExclusivePlayback) {
        if let active, active !== player {
            active.standDown()
        }
        active = player
    }

    /// Releases playback, but only when `player` is still the active one, so a screen
    /// going away cannot clear a claim that a newer one already holds.
    func deactivate(_ player: any ExclusivePlayback) {
        if active === player { active = nil }
    }

    /// Pauses the active player, for callers that are not players themselves.
    func pause() {
        active?.standDown()
    }
}

extension PlayerEngine: ExclusivePlayback {
    func standDown() { pause() }
}

extension MpvEngine: ExclusivePlayback {
    func standDown() { pause() }
}
