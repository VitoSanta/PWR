public enum EvictionReason: Equatable {
    case capacity, expired, replaced, removed
}

public final class LRUCache<Key: Hashable, Value> {
    private final class Node {
        let key: Key
        var value: Value
        var expires: Double?
        var newer: Node?
        weak var older: Node?

        init(key: Key, value: Value, expires: Double?) {
            self.key = key
            self.value = value
            self.expires = expires
        }
    }

    private let capacity: Int
    private let now: () -> Double
    private var nodes: [Key: Node] = [:]
    private var newest: Node?
    private var oldest: Node?

    public var onEvict: ((Key, Value, EvictionReason) -> Void)?

    public init(capacity: Int, now: @escaping () -> Double) {
        precondition(capacity >= 1, "capacity must be at least 1")
        self.capacity = capacity
        self.now = now
    }

    private func unlink(_ node: Node) {
        if let newer = node.newer { newer.older = node.older } else { newest = node.older }
        if let older = node.older { older.newer = node.newer } else { oldest = node.newer }
        node.newer = nil
        node.older = nil
    }

    private func pushNewest(_ node: Node) {
        node.older = newest
        node.newer = nil
        newest?.newer = node
        newest = node
        if oldest == nil { oldest = node }
    }

    private func isExpired(_ node: Node) -> Bool {
        node.expires.map { $0 <= now() } ?? false
    }

    private func drop(_ node: Node, _ reason: EvictionReason) {
        unlink(node)
        nodes[node.key] = nil
        onEvict?(node.key, node.value, reason)
    }

    private func sweep() {
        var node = oldest
        while let current = node {
            node = current.newer
            if isExpired(current) { drop(current, .expired) }
        }
    }

    private func live(_ key: Key) -> Node? {
        guard let node = nodes[key] else { return nil }
        if isExpired(node) {
            drop(node, .expired)
            return nil
        }
        return node
    }

    public func get(_ key: Key) -> Value? {
        guard let node = live(key) else { return nil }
        unlink(node)
        pushNewest(node)
        return node.value
    }

    public func set(_ key: Key, _ value: Value, ttl: Double? = nil) {
        let expires = ttl.map { now() + $0 }
        if let node = live(key) {
            let old = node.value
            node.value = value
            node.expires = expires
            unlink(node)
            pushNewest(node)
            onEvict?(key, old, .replaced)
            return
        }
        if nodes.count >= capacity { sweep() }
        if nodes.count >= capacity, let victim = oldest { drop(victim, .capacity) }
        let node = Node(key: key, value: value, expires: expires)
        nodes[key] = node
        pushNewest(node)
    }

    @discardableResult
    public func remove(_ key: Key) -> Value? {
        guard let node = live(key) else { return nil }
        drop(node, .removed)
        return node.value
    }

    public var count: Int {
        sweep()
        return nodes.count
    }

    public var keys: [Key] {
        sweep()
        var result: [Key] = []
        var node = newest
        while let current = node {
            result.append(current.key)
            node = current.older
        }
        return result
    }
}
