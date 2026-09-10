import Foundation

/// Views derived from the core's snapshot: saved titles, Continue watching and the
/// next episode each series should resume.
///
/// The masking rules live here rather than in the screens, so Home, Library and the
/// player all agree about what counts as "in progress".

/// True when two keys address the same title.
func sameKey(_ left: JSONValue?, _ right: JSONValue) -> Bool {
    guard let left else { return false }
    return ["installation_id", "content_type", "item_id"].allSatisfy { left.text($0) == right.text($0) }
}

/// Titles the user saved, in saved order. Entries whose metadata has not been
/// fetched yet still appear, using the stored title text.
func savedTitles(_ snapshot: JSONValue) -> [Title] {
    snapshot.objects("library").map { entry in
        let key = entry["key"] ?? .object([:])
        return Title(
            provider: key.text("installation_id"),
            raw: entry["metadata"] ?? [
                "id": .string(key.text("item_id")),
                "type": .string(key.text("content_type")),
                "name": .string(entry.text("title")),
            ]
        )
    }
}

/// One Continue watching card: cached metadata plus the video the core selected.
struct ContinueEntry: Sendable {
    let title: Title
    let meta: JSONValue
    let hasVideos: Bool
    /// The next unwatched episode, or nil when the series is finished.
    let episode: JSONValue?

    /// A series with episodes but no next episode has been watched to the end and
    /// is hidden instead of being offered again.
    var isFinished: Bool { title.isSeries && hasVideos && episode == nil }
}

/// In-progress titles, most recently watched first.
func continueTitles(_ snapshot: JSONValue) -> [Title] {
    let hidden = snapshot.objects("hidden_continue")
    return snapshot.objects("progress").reversed()
        .filter { progress in
            let key = progress["key"] ?? .object([:])
            // Finished movies drop out; a finished series may still have a next episode.
            let keep = !progress.boolean("completed") || key.text("content_type") == "series"
            return keep
                && progress.number("position_ms") > 0
                && !hidden.contains { sameKey($0, key) }
        }
        .map { entry in
            let key = entry["key"] ?? .object([:])
            return Title(
                provider: key.text("installation_id"),
                raw: entry["metadata"] ?? [
                    "id": .string(key.text("item_id")),
                    "type": .string(key.text("content_type")),
                    "name": .string(key.text("item_id")),
                ]
            )
        }
        .reduce(into: [Title]()) { unique, title in
            if !unique.contains(where: { $0.identity == title.identity }) { unique.append(title) }
        }
}

/// The stored progress entry for the video the core selected.
///
/// Matching on the chosen video means an unwatched next episode shows no bar
/// instead of the previous episode's position.
func progressFor(_ snapshot: JSONValue, title: Title, videoId: String?) -> JSONValue? {
    let history = snapshot.objects("progress").filter { sameKey($0["key"], title.key) }
    guard let videoId, !videoId.isEmpty else { return history.last }
    return history.last { $0.text("video_id") == videoId }
}

func progressFraction(_ progress: JSONValue?) -> Double {
    guard let progress else { return 0 }
    let duration = max(progress.number("duration_ms"), 1)
    return min(max(progress.number("position_ms") / duration, 0), 1)
}

/// Minutes left, rounded up, for the "Resume · N min left" label.
func minutesLeft(_ progress: JSONValue) -> Int {
    let remaining = max(progress.number("duration_ms") - progress.number("position_ms"), 0)
    return Int((remaining + 59_999) / 60_000)
}
