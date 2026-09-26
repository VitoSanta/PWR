const std = @import("std");

/// `zig build test` runs the module's own tests and every `tests/*.zig` file,
/// each importing the library as `duration`.
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const duration = b.addModule("duration", .{
        .root_source_file = b.path("src/duration.zig"),
        .target = target,
        .optimize = optimize,
    });
    const test_step = b.step("test", "Run the tests");
    test_step.dependOn(&b.addRunArtifact(b.addTest(.{ .root_module = duration })).step);

    const io = b.graph.io;
    var dir = b.build_root.handle.openDir(io, "tests", .{ .iterate = true }) catch return;
    defer dir.close(io);
    var names: std.ArrayList([]const u8) = .empty;
    var entries = dir.iterate();
    while (entries.next(io) catch null) |entry| {
        if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".zig")) {
            names.append(b.allocator, b.dupe(entry.name)) catch @panic("out of memory");
        }
    }
    std.mem.sort([]const u8, names.items, {}, struct {
        fn less(_: void, a: []const u8, c: []const u8) bool {
            return std.mem.lessThan(u8, a, c);
        }
    }.less);
    for (names.items) |name| {
        const tests = b.addTest(.{ .root_module = b.createModule(.{
            .root_source_file = b.path(b.fmt("tests/{s}", .{name})),
            .target = target,
            .optimize = optimize,
            .imports = &.{.{ .name = "duration", .module = duration }},
        }) });
        test_step.dependOn(&b.addRunArtifact(tests).step);
    }
}
