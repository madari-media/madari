import Foundation

/// A JSON value that can cross concurrency domains.
///
/// Addon manifests, catalog pages and snapshots are open-ended documents, so the
/// client keeps them as JSON instead of mirroring the Rust types. `JSONValue` is a
/// `Sendable` replacement for the `JSONObject`/`JSONArray` pair the Android client
/// uses, which lets every native call be awaited from any task.
///
/// Encoding is hand-written rather than synthesised: `JSONEncoder` drops keys whose
/// value is null, and a present-but-null field is not always the same as an absent
/// one to the core.
enum JSONValue: Sendable, Hashable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])
}

// MARK: - Literals

extension JSONValue: ExpressibleByNilLiteral {
    init(nilLiteral: ()) { self = .null }
}
extension JSONValue: ExpressibleByBooleanLiteral {
    init(booleanLiteral value: Bool) { self = .bool(value) }
}
extension JSONValue: ExpressibleByIntegerLiteral {
    init(integerLiteral value: Int) { self = .number(Double(value)) }
}
extension JSONValue: ExpressibleByFloatLiteral {
    init(floatLiteral value: Double) { self = .number(value) }
}
extension JSONValue: ExpressibleByStringLiteral {
    init(stringLiteral value: String) { self = .string(value) }
}
extension JSONValue: ExpressibleByArrayLiteral {
    init(arrayLiteral elements: JSONValue...) { self = .array(elements) }
}
extension JSONValue: ExpressibleByDictionaryLiteral {
    init(dictionaryLiteral elements: (String, JSONValue)...) {
        self = .object(Dictionary(elements, uniquingKeysWith: { _, last in last }))
    }
}

// MARK: - Reading

extension JSONValue {
    /// The object's entries, or an empty map when this is not an object.
    var fields: [String: JSONValue] {
        if case .object(let fields) = self { return fields }
        return [:]
    }
    var elements: [JSONValue] {
        if case .array(let elements) = self { return elements }
        return []
    }
    /// An absent key and an explicit null both read as nil, matching the Android helpers.
    ///
    /// Settable so request payloads can be built up in place, as the Android client
    /// does with `JSONObject.put`.
    subscript(key: String) -> JSONValue? {
        get {
            guard let value = fields[key], value != .null else { return nil }
            return value
        }
        set {
            var fields = self.fields
            if let newValue {
                fields[key] = newValue
            } else {
                fields.removeValue(forKey: key)
            }
            self = .object(fields)
        }
    }
    var isNull: Bool { self == .null }
    var stringValue: String {
        if case .string(let value) = self { return value }
        return ""
    }
    var doubleValue: Double {
        if case .number(let value) = self { return value }
        return 0
    }
    var intValue: Int {
        if case .number(let value) = self { return Int(value) }
        return 0
    }
    var boolValue: Bool {
        if case .bool(let value) = self { return value }
        return false
    }
    /// Matches `JSONObject.text`: missing, null and wrong-typed values all read empty.
    func text(_ key: String) -> String { self[key]?.stringValue ?? "" }
    func number(_ key: String) -> Double { self[key]?.doubleValue ?? 0 }
    func integer(_ key: String) -> Int { self[key]?.intValue ?? 0 }
    func boolean(_ key: String) -> Bool { self[key]?.boolValue ?? false }
    /// A string array field, tolerating a missing key.
    func strings(_ key: String) -> [String] {
        self[key]?.elements.compactMap { element in
            if case .string(let value) = element { return value }
            return nil
        } ?? []
    }
    /// The object elements of an array field; non-objects are skipped.
    func objects(_ key: String) -> [JSONValue] {
        self[key]?.elements.filter { if case .object = $0 { return true } else { return false } } ?? []
    }
    /// `ForEach` keys, as computed properties because key paths cannot call methods.
    /// Addon installations and every other identified object use different fields,
    /// and picking the wrong one collapses a list to a single row.
    ///
    /// The addon installation id: addons, and the options that address them.
    var installationID: String { text("installation_id") }
    /// The item id: titles, videos, episodes, profiles and avatars.
    var itemID: String { text("id") }
    /// The URL field, for lists keyed by address rather than id.
    var entryURL: String { text("url") }
}

// MARK: - Decoding

extension JSONValue: Decodable {
    init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([JSONValue].self) {
            self = .array(value)
        } else if let value = try? container.decode([String: JSONValue].self) {
            self = .object(value)
        } else {
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Unsupported JSON value"
            )
        }
    }
}

extension JSONValue {
    /// Parses one of the core's JSON responses.
    static func parse(_ text: String) throws -> JSONValue {
        guard let data = text.data(using: .utf8) else {
            throw JSONError.malformed
        }
        return try JSONDecoder().decode(JSONValue.self, from: data)
    }
}

enum JSONError: Error, LocalizedError {
    case malformed

    var errorDescription: String? {
        switch self {
        case .malformed: "The native core returned an unreadable response."
        }
    }
}

// MARK: - Encoding

extension JSONValue {
    /// Renders this value as JSON text, keeping explicit nulls.
    func serialized() -> String {
        var out = ""
        write(into: &out)
        return out
    }

    private func write(into out: inout String) {
        switch self {
        case .null:
            out += "null"
        case .bool(let value):
            out += value ? "true" : "false"
        case .number(let value):
            // Whole numbers must not serialise as `1.0`; the core reads some fields
            // as integers and rejects a fractional representation.
            if value.rounded() == value, abs(value) < 1e15 {
                out += String(Int64(value))
            } else {
                out += String(value)
            }
        case .string(let value):
            JSONValue.writeString(value, into: &out)
        case .array(let elements):
            out += "["
            for (index, element) in elements.enumerated() {
                if index > 0 { out += "," }
                element.write(into: &out)
            }
            out += "]"
        case .object(let fields):
            out += "{"
            var first = true
            for key in fields.keys.sorted() {
                guard let value = fields[key] else { continue }
                if !first { out += "," }
                first = false
                JSONValue.writeString(key, into: &out)
                out += ":"
                value.write(into: &out)
            }
            out += "}"
        }
    }

    private static func writeString(_ value: String, into out: inout String) {
        out += "\""
        for character in value.unicodeScalars {
            switch character {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            default:
                if character.value < 0x20 {
                    out += String(format: "\\u%04x", character.value)
                } else {
                    out.unicodeScalars.append(character)
                }
            }
        }
        out += "\""
    }
}
