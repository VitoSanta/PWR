import XCTest
@testable import Cache

final class LRUCacheTests: XCTestCase {
    var time = 0.0
    var evicted: [(String, Int, EvictionReason)] = []

    func make(_ capacity: Int) -> LRUCache<String, Int> {
        let cache = LRUCache<String, Int>(capacity: capacity, now: { [unowned self] in self.time })
        cache.onEvict = { [unowned self] key, value, reason in self.evicted.append((key, value, reason)) }
        return cache
    }

    func testEvictsLeastRecentlyUsed() {
        let cache = make(2)
        cache.set("a", 1)
        cache.set("b", 2)
        XCTAssertEqual(cache.get("a"), 1)
        cache.set("c", 3)
        XCTAssertNil(cache.get("b"))
        XCTAssertEqual(cache.keys, ["c", "a"])
        XCTAssertEqual(evicted.map { $0.0 }, ["b"])
        XCTAssertEqual(evicted.map { $0.2 }, [.capacity])
    }

    func testReplacingRefreshesAndReports() {
        let cache = make(2)
        cache.set("a", 1)
        cache.set("b", 2)
        cache.set("a", 10)
        cache.set("c", 3)
        XCTAssertEqual(cache.keys, ["c", "a"])
        XCTAssertEqual(cache.get("a"), 10)
        XCTAssertEqual(evicted.map { "\($0.0)=\($0.1):\($0.2)" }, ["a=1:replaced", "b=2:capacity"])
    }

    func testExpiry() {
        let cache = make(3)
        cache.set("short", 1, ttl: 5)
        cache.set("long", 2, ttl: 60)
        cache.set("forever", 3)
        time = 4.999
        XCTAssertEqual(cache.get("short"), 1)
        time = 5
        XCTAssertNil(cache.get("short"))
        XCTAssertEqual(cache.count, 2)
        XCTAssertEqual(evicted.map { "\($0.0):\($0.2)" }, ["short:expired"])
        time = 100
        XCTAssertEqual(cache.keys, ["forever"])
        XCTAssertEqual(cache.count, 1)
    }

    func testExpiredEntriesGoBeforeLiveOnes() {
        let cache = make(2)
        cache.set("old", 1)
        cache.set("temp", 2, ttl: 1)
        _ = cache.get("temp")
        time = 2
        cache.set("new", 3)
        XCTAssertEqual(cache.keys, ["new", "old"])
        XCTAssertEqual(evicted.map { "\($0.0):\($0.2)" }, ["temp:expired"])
    }

    func testRemove() {
        let cache = make(2)
        cache.set("a", 1)
        XCTAssertEqual(cache.remove("a"), 1)
        XCTAssertNil(cache.remove("a"))
        XCTAssertEqual(cache.count, 0)
        XCTAssertEqual(evicted.map { "\($0.0):\($0.2)" }, ["a:removed"])
    }
}
