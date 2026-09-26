const std = @import("std");
const duration = @import("duration");

const expectEqual = std.testing.expectEqual;
const expectError = std.testing.expectError;
const expectEqualStrings = std.testing.expectEqualStrings;
const max = std.math.maxInt(u64);

test "hidden: the first problem, in the stated order" {
    try expectError(error.MissingUnit, duration.parse("99999999999999999999"));
    try expectError(error.UnknownUnit, duration.parse("99999999999999999999y"));
    try expectError(error.UnitOrder, duration.parse("1s99999999999999999999h"));
    try expectError(error.UnknownUnit, duration.parse("1h5y"));
    try expectError(error.UnknownUnit, duration.parse("5y1h"));
    try expectError(error.UnknownUnit, duration.parse("1hh"));
    try expectError(error.UnitOrder, duration.parse("1ms5s"));
    try expectError(error.MissingNumber, duration.parse("5s!"));
}

test "hidden: leading zeros, however many" {
    try expectEqual(@as(u64, 1_000), try duration.parse("000000000000000000000000001s"));
}

test "hidden: the edge of u64" {
    var buf: [64]u8 = undefined;
    try expectEqualStrings("30500568904w6d14h25m51s615ms", try duration.format(max, &buf));
    try expectEqual(@as(u64, max), try duration.parse("30500568904w6d14h25m51s615ms"));
    try expectEqual(@as(u64, 18_446_744_073_139_200_000), try duration.parse("30500568904w"));
    try expectError(error.Overflow, duration.parse("30500568905w"));
    try expectError(error.Overflow, duration.parse("30500568904w7d"));
}

test "hidden: a buffer too small, and one exactly large enough" {
    var buf: [64]u8 = undefined;
    try expectError(error.NoSpaceLeft, duration.format(5_400_000, buf[0..3]));
    try expectEqualStrings("1h30m", try duration.format(5_400_000, buf[0..5]));
    try expectError(error.NoSpaceLeft, duration.format(0, buf[0..2]));
}

test "hidden: round trip over many values" {
    var buf: [64]u8 = undefined;
    var prng = std.Random.DefaultPrng.init(0x5eed);
    const random = prng.random();
    var i: usize = 0;
    while (i < 2000) : (i += 1) {
        const value = switch (i % 3) {
            0 => random.int(u64),
            1 => random.uintLessThan(u64, 1_000_000_000),
            else => random.uintLessThan(u64, 100_000),
        };
        try expectEqual(value, try duration.parse(try duration.format(value, &buf)));
    }
}
