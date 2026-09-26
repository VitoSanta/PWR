//! Durations such as `1h30m`: see README.md.

pub const ParseError = error{ Empty, MissingNumber, MissingUnit, UnknownUnit, UnitOrder, Overflow };

pub fn parse(text: []const u8) ParseError!u64 {
    _ = text;
    return error.Empty;
}

pub fn format(ms: u64, buf: []u8) error{NoSpaceLeft}![]const u8 {
    _ = ms;
    _ = buf;
    return error.NoSpaceLeft;
}
