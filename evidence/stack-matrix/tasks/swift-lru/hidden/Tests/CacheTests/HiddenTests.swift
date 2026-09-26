import XCTest
@testable import Cache

final class HiddenTests: XCTestCase {
    func testMatchesASimpleModel() {
        var time = 0.0
        let cache = LRUCache<Int, Int>(capacity: 7, now: { time })
        var model: [(key: Int, value: Int, expires: Double?)] = []   // most recent first
        var generator = SystemRandomNumberGenerator()
        _ = generator
        var seed: UInt64 = 42
        func next(_ bound: Int) -> Int {
            seed = seed &* 6364136223846793005 &+ 1442695040888963407
            return Int((seed >> 33) % UInt64(bound))
        }
        func sweep() { model.removeAll { $0.expires.map { $0 <= time } ?? false } }
        for step in 0..<5000 {
            time += Double(next(3))
            let key = next(12)
            switch next(4) {
            case 0:
                sweep()
                let ttl: Double? = next(2) == 0 ? nil : Double(next(10) + 1)
                model.removeAll { $0.key == key }
                model.insert((key, step, ttl.map { time + $0 }), at: 0)
                if model.count > 7 { model.removeLast() }
                cache.set(key, step, ttl: ttl)
            case 1:
                sweep()
                let expected = model.first { $0.key == key }?.value
                if let index = model.firstIndex(where: { $0.key == key }) {
                    let entry = model.remove(at: index)
                    model.insert(entry, at: 0)
                }
                XCTAssertEqual(cache.get(key), expected, "step \(step)")
            case 2:
                sweep()
                let expected = model.first { $0.key == key }?.value
                model.removeAll { $0.key == key }
                XCTAssertEqual(cache.remove(key), expected, "step \(step)")
            default:
                sweep()
                XCTAssertEqual(cache.keys, model.map { $0.key }, "step \(step)")
                XCTAssertEqual(cache.count, model.count, "step \(step)")
            }
        }
    }

    func testManyOperationsAreFast() {
        let cache = LRUCache<Int, Int>(capacity: 1000, now: { 0 })
        let start = Date()
        for i in 0..<200_000 {
            cache.set(i % 5000, i)
            _ = cache.get((i * 7) % 5000)
        }
        XCTAssertLessThan(Date().timeIntervalSince(start), 20)
        XCTAssertEqual(cache.count, 1000)
    }
}
