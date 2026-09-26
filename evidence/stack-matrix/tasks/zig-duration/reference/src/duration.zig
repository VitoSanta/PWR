//! Durations such as `1h30m`: see README.md.
const std = @import("std");

pub const ParseError = error{ Empty, MissingNumber, MissingUnit, UnknownUnit, UnitOrder, Overflow };

const Unit = struct { name: []const u8, ms: u64 };

/// Largest first: a unit's index is its rank, and order means rising index.
const units = [_]Unit{
    .{ .name = "w", .ms = 604_800_000 },
    .{ .name = "d", .ms = 86_400_000 },
    .{ .name = "h", .ms = 3_600_000 },
    .{ .name = "m", .ms = 60_000 },
    .{ .name = "s", .ms = 1_000 },
    .{ .name = "ms", .ms = 1 },
};

fn unitIndex(name: []const u8) ?usize {
    for (units, 0..) |unit, index| {
        if (std.mem.eql(u8, unit.name, name)) return index;
    }
    return null;
}

pub fn parse(text: []const u8) ParseError!u64 {
    if (text.len == 0) return error.Empty;
    var total: u64 = 0;
    var previous: ?usize = null;
    var i: usize = 0;
    while (i < text.len) {
        const digits_start = i;
        // The value, with overflow noted rather than raised: a segment is
        // only an overflow once it is otherwise well formed.
        var value: u64 = 0;
        var too_large = false;
        while (i < text.len and std.ascii.isDigit(text[i])) : (i += 1) {
            if (!too_large) {
                const times = @mulWithOverflow(value, 10);
                const plus = @addWithOverflow(times[0], text[i] - '0');
                if (times[1] != 0 or plus[1] != 0) too_large = true else value = plus[0];
            }
        }
        if (i == digits_start) return error.MissingNumber;
        const unit_start = i;
        while (i < text.len and std.ascii.isAlphabetic(text[i])) : (i += 1) {}
        if (i == unit_start) return error.MissingUnit;
        const index = unitIndex(text[unit_start..i]) orelse return error.UnknownUnit;
        if (previous) |before| {
            if (index <= before) return error.UnitOrder;
        }
        if (too_large) return error.Overflow;
        const scaled = @mulWithOverflow(value, units[index].ms);
        if (scaled[1] != 0) return error.Overflow;
        const sum = @addWithOverflow(total, scaled[0]);
        if (sum[1] != 0) return error.Overflow;
        total = sum[0];
        previous = index;
    }
    return total;
}

pub fn format(ms: u64, buf: []u8) error{NoSpaceLeft}![]const u8 {
    if (ms == 0) {
        if (buf.len < 3) return error.NoSpaceLeft;
        @memcpy(buf[0..3], "0ms");
        return buf[0..3];
    }
    var rest = ms;
    var written: usize = 0;
    for (units) |unit| {
        const count = rest / unit.ms;
        rest %= unit.ms;
        if (count == 0) continue;
        const part = std.fmt.bufPrint(buf[written..], "{d}{s}", .{ count, unit.name }) catch
            return error.NoSpaceLeft;
        written += part.len;
    }
    return buf[0..written];
}

test "a week and a millisecond" {
    var buf: [16]u8 = undefined;
    try std.testing.expectEqualStrings("1w1ms", try format(604_800_001, &buf));
}
