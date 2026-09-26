# Cache

A least-recently-used cache with optional per-entry expiry, as a Swift
package (Swift 5.9+, macOS and Linux, Foundation not required). Module
`Cache`:

```swift
public enum EvictionReason: Equatable { case capacity, expired, replaced, removed }

public final class LRUCache<Key: Hashable, Value> {
    public init(capacity: Int, now: @escaping () -> Double)   // seconds; capacity >= 1 (precondition)
    public var onEvict: ((Key, Value, EvictionReason) -> Void)?

    public func get(_ key: Key) -> Value?
    public func set(_ key: Key, _ value: Value, ttl: Double? = nil)
    @discardableResult public func remove(_ key: Key) -> Value?
    public var count: Int { get }
    public var keys: [Key] { get }
}
```

- `set` stores the value as the most recently used. Replacing a key's value
  reports the old one to `onEvict` as `.replaced`. When a new key would make
  the cache hold more than `capacity` entries, the least recently used one
  goes, reported as `.capacity` -- after expired entries, which go first
  (reported as `.expired`).
- `ttl` (seconds) makes the entry expire at `now() + ttl`: from that moment it
  is as if it were not there. An expired entry is removed, and reported as
  `.expired`, when anything touches it or needs its room -- `get`, `set`,
  `count`, `keys` -- never later than that.
- `get` returns the value and makes the entry the most recently used; `nil`
  for a missing or expired key.
- `remove` returns the value (`nil` if missing or expired) and reports it as
  `.removed`.
- `count` and `keys` never include expired entries; `keys` is ordered from
  most to least recently used.
- Operations are O(1) on average (a dictionary and a linked list), apart
  from `keys` and the sweep of expired entries.

Run the tests with `swift test`.
