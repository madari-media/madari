import SwiftUI

/// One managed torrent, as the core's `torrents` operation reports it.
struct ManagedTorrent: Identifiable, Hashable {
    let id: String
    let name: String
    let state: String
    let downloaded: Double
    let total: Double
    let downloadSpeed: Double
    let peers: Int
    let path: String

    init?(_ value: JSONValue) {
        let id = value.text("id")
        guard !id.isEmpty else { return nil }
        self.id = id
        name = value.text("name").nilIfEmpty ?? id
        state = value.text("state")
        downloaded = value["downloaded"]?.doubleValue ?? 0
        total = value["total"]?.doubleValue ?? 0
        downloadSpeed = value["download_speed"]?.doubleValue ?? 0
        peers = Int(value["peers"]?.doubleValue ?? 0)
        path = value.text("path")
    }

    var progress: Double {
        guard total > 0 else { return 0 }
        return min(1, max(0, downloaded / total))
    }

    var isComplete: Bool { total > 0 && downloaded >= total }

    /// The file name as it exists on disk, which is what the player shows as the title.
    var fileName: String {
        let last = name.split(separator: "/").last.map(String.init) ?? name
        return last.nilIfEmpty ?? name
    }

    var detail: String {
        if isComplete { return "Downloaded" }
        let percent = Int((progress * 100).rounded())
        let speed = ByteCountFormatter.string(fromByteCount: Int64(downloadSpeed), countStyle: .file)
        return "\(percent)% · \(speed)/s · \(peers) peer\(peers == 1 ? "" : "s")"
    }
}

/// The torrents page: what is downloading on this device, with a way to play a file
/// straight away.
///
/// Playing goes through the core's `torrent_play`, which also tells the torrent engine to
/// prioritise that file — without it, reads of pieces that have not arrived yet block
/// until the download happens to reach them.
struct TorrentsView: View {
    @EnvironmentObject private var model: AppModel

    @State private var torrents: [ManagedTorrent] = []
    @State private var loading = true
    @State private var failure: String?

    var body: some View {
        MadariScreen {
            List {
                if let failure {
                    Text(failure).madariBody().foregroundStyle(MadariColors.muted)
                }
                if torrents.isEmpty, !loading, failure == nil {
                    Text("Nothing is downloading right now.")
                        .madariBody()
                        .foregroundStyle(MadariColors.muted)
                }
                ForEach(torrents) { torrent in
                    row(torrent)
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .overlay { if loading { ProgressView().tint(MadariColors.accent) } }
        }
        .navigationTitle("Torrents")
        .task { await refresh() }
        .refreshable { await refresh() }
    }

    private func row(_ torrent: ManagedTorrent) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(torrent.fileName)
                .font(MadariFont.medium(14))
                .lineLimit(2)
            ProgressView(value: torrent.progress)
                .tint(torrent.isComplete ? MadariColors.accent : .white)
            HStack {
                Text(torrent.detail)
                    .font(MadariFont.regular(11))
                    .foregroundStyle(MadariColors.muted)
                Spacer()
                Button("Play") { Task { await play(torrent) } }
                    .buttonStyle(QuietButton())
            }
        }
        .padding(.vertical, 4)
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) {
                Task { await remove(torrent) }
            } label: {
                Label("Remove", systemImage: "trash")
            }
        }
    }

    private func refresh() async {
        torrents = await model.torrents()
        loading = false
    }

    private func play(_ torrent: ManagedTorrent) async {
        failure = nil
        guard let playback = await model.openTorrent(torrent) else {
            failure = "That torrent could not be opened for playback."
            return
        }
        model.state.playback = playback
    }

    private func remove(_ torrent: ManagedTorrent) async {
        await model.removeTorrent(id: torrent.id)
        await refresh()
    }
}

/// The Home entry point for the torrents page.
struct TorrentsRow: View {
    var onOpen: () -> Void

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 12) {
                GlyphIcon(glyph: .torrent, size: 18)
                    .foregroundStyle(MadariColors.accent)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Torrents").font(MadariFont.medium(15))
                    Text("Play what is downloading")
                        .font(MadariFont.regular(12))
                        .foregroundStyle(MadariColors.muted)
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(MadariColors.muted)
            }
            .padding(14)
            .background(MadariColors.panel, in: RoundedRectangle(cornerRadius: 14))
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 16)
    }
}
