import SwiftUI

/// The TV client draws a hand-built stroke glyph set because tvOS has no icon font.
/// iOS has SF Symbols, which are sharper, scale with Dynamic Type and are already
/// understood by users, so the same concepts are mapped onto them here.
enum Glyph: String {
    case home = "house"
    case explore = "square.grid.2x2"
    case library = "bookmark"
    case search = "magnifyingglass"
    case calendar = "calendar"
    case settings = "gearshape"
    case play = "play.fill"
    case pause = "pause.fill"
    case next = "forward.end.fill"
    case previous = "backward.end.fill"
    case backward = "gobackward.10"
    case forward = "goforward.10"
    case audio = "speaker.wave.2"
    case subtitles = "captions.bubble"
    case episodes = "list.bullet.rectangle"
    case screen = "rectangle.inset.filled"
    case speed = "speedometer"
    case source = "square.stack.3d.up"
    case check = "checkmark"
    case plus = "plus"
    case info = "info.circle"
    case close = "xmark"
    case lock = "lock.fill"
    case profile = "person.crop.circle"
    case refresh = "arrow.clockwise"
    case trash = "trash"
    case share = "person.badge.plus"
    case sliders = "slider.horizontal.3"
    case globe = "globe"
    case torrent = "arrow.down.circle"
}

/// A glyph at a consistent optical weight.
struct GlyphIcon: View {
    let glyph: Glyph
    var size: CGFloat = 17

    var body: some View {
        Image(systemName: glyph.rawValue)
            .font(.system(size: size, weight: .medium))
    }
}
