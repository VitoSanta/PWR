const std = @import("std");
const duration = @import("duration");

const expectEqual = std.testing.expectEqual;
const expectError = std.testing.expectError;
const expectEqualStrings = std.testing.expectEqualStrings;

test "one unit" {
    try expectEqual(@as(u64, 250), try duration.parse("250ms"));
    try expectEqual(@as(u64, 90_000), try duration.parse("90s"));
    try expectEqual(@as(u64, 300_000), try duration.parse("5m"));
    try expectEqual(@as(u64, 7_200_000), try duration.parse("2h"));
    try expectEqual(@as(u64, 86_400_000), try duration.parse("1d"));
    try expectEqual(@as(u64, 604_800_000), try duration.parse("1w"));
}

test "several segments, largest first" {
    try expectEqual(@as(u64, 5_400_000), try duration.parse("1h30m"));
    try expectEqual(@as(u64, 788_645_006), try duration.parse("1w2d3h4m5s6ms"));
    try expectEqual(@as(u64, 65_000), try duration.parse("1m5s"));
}

test "zeros" {
    try expectEqual(@as(u64, 0), try duration.parse("0s"));
    try expectEqual(@as(u64, 420_000), try duration.parse("007m"));
    try expectEqual(@as(u64, 0), try duration.parse("0h0m"));
}

test "malformed text" {
    try expectError(error.Empty, duration.parse(""));
    try expectError(error.MissingNumber, duration.parse("h"));
    try expectError(error.MissingNumber, duration.parse("-5s"));
    try expectError(error.MissingNumber, duration.parse("1h 30m"));
    try expectError(error.MissingUnit, duration.parse("10"));
    try expectError(error.MissingUnit, duration.parse("1h30"));
    try expectError(error.UnknownUnit, duration.parse("5y"));
    try expectError(error.UnknownUnit, duration.parse("5sec"));
    try expectError(error.UnknownUnit, duration.parse("1H"));
}

test "units in decreasing order" {
    try expectError(error.UnitOrder, duration.parse("30m1h"));
    try expectError(error.UnitOrder, duration.parse("1h1h"));
}

test "overflow" {
    try expectError(error.Overflow, duration.parse("99999999999999999999ms"));
}

test "format" {
    var buf: [64]u8 = undefined;
    try expectEqualStrings("0ms", try duration.format(0, &buf));
    try expectEqualStrings("1s", try duration.format(1_000, &buf));
    try expectEqualStrings("59s999ms", try duration.format(59_999, &buf));
    try expectEqualStrings("1h30m", try duration.format(5_400_000, &buf));
    try expectEqualStrings("1w1ms", try duration.format(604_800_001, &buf));
    try expectEqualStrings("1d1h1m1s1ms", try duration.format(90_061_001, &buf));
}

test "round trip" {
    var buf: [64]u8 = undefined;
    for ([_]u64{ 0, 1, 999, 60_000, 5_400_000, 788_645_006 }) |value| {
        try expectEqual(value, try duration.parse(try duration.format(value, &buf)));
    }
}
