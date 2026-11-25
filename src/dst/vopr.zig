const std = @import("std");
const tiger = @import("../tiger.zig");
const sim_storage = @import("sim_storage.zig");
const sim_fleet = @import("sim_fleet.zig");
const sim_http = @import("sim_http.zig");
const sim_node = @import("sim_node.zig");
const oracle = @import("oracle.zig");

var panic_seed: ?u64 = null;
var panic_steps: usize = 0;
var panic_fleet_long = false;

pub fn panic_handler(message: []const u8, address: ?usize) noreturn {
    sim_fleet.print_panic_trace();
    if (panic_seed) |seed| {
        std.debug.print("FAIL panic seed={d}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose{s}\n", .{ seed, seed, panic_steps, if (panic_fleet_long) " --fleet-long" else "" });
    }
    std.debug.defaultPanic(message, address);
}

pub fn main(init: std.process.Init) !u8 {
    var arena_st = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_st.deinit();
    const arena = arena_st.allocator();

    const args = try init.minimal.args.toSlice(arena);
    var seeds: u64 = 100;
    var steps: usize = 400;
    var single_seed: ?u64 = null;
    var verbose = false;
    var fleet_long = false;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        if (std.mem.eql(u8, args[i], "--seeds") and i + 1 < args.len) {
            i += 1;
            seeds = try std.fmt.parseInt(u64, args[i], 10);
        } else if (std.mem.eql(u8, args[i], "--steps") and i + 1 < args.len) {
            i += 1;
            steps = try std.fmt.parseInt(usize, args[i], 10);
        } else if (std.mem.eql(u8, args[i], "--seed") and i + 1 < args.len) {
            i += 1;
            single_seed = try std.fmt.parseInt(u64, args[i], 10);
        } else if (std.mem.eql(u8, args[i], "--verbose")) {
            verbose = true;
        } else if (std.mem.eql(u8, args[i], "--fleet-long")) {
            fleet_long = true;
        }
    }

    const start_seed: u64 = single_seed orelse 0;
    const end_seed: u64 = if (single_seed) |s| s + 1 else seeds;
    panic_steps = steps;
    panic_fleet_long = fleet_long;

    var out_buf: [4096]u8 = undefined;
    var out = std.Io.File.stdout().writer(init.io, &out_buf);

    var s: u64 = start_seed;
    while (s < end_seed) : (s += 1) {
        panic_seed = s;
        var dbg: std.heap.DebugAllocator(.{}) = .init;
        const gpa = dbg.allocator();

        sim_storage.run_storage_sim(gpa, s ^ 0x9e3779b97f4a7c15, steps) catch |err| {
            std.debug.print("FAIL storage seed={d} err={s}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose\n", .{ s, @errorName(err), s, steps });
            return 1;
        };
        sim_fleet.run(gpa, s ^ 0x6a09e667f3bcc909, fleet_long) catch |err| {
            std.debug.print("FAIL fleet seed={d} err={s}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose{s}\n", .{ s, @errorName(err), s, steps, if (fleet_long) " --fleet-long" else "" });
            return 1;
        };
        sim_http.run(gpa, s ^ 0x3c6ef372fe94f82b) catch |err| {
            std.debug.print("FAIL http seed={d} err={s}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose\n", .{ s, @errorName(err), s, steps });
            return 1;
        };
        sim_node.run(gpa, s ^ 0x3c6ef372fe94f82b) catch |err| {
            std.debug.print("FAIL node seed={d} err={s}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose\n", .{ s, @errorName(err), s, steps });
            return 1;
        };
        oracle.run_seed(gpa, s, steps) catch |err| {
            std.debug.print("FAIL oracle seed={d} err={s}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose\n", .{ s, @errorName(err), s, steps });
            return 1;
        };

        if (dbg.deinit() == .leak) {
            std.debug.print("FAIL memory leak on seed={d}\nRepro: zig build dst -- --seed {d} --steps {d} --verbose\n", .{ s, s, steps });
            return 1;
        }

        if (verbose and ((s % 25) == 0 or s + 1 == end_seed)) {
            try out.interface.print("vopr: seed {d}/{d} ok ({d} steps)\n", .{ s + 1, end_seed, steps });
            try out.interface.flush();
        }
    }

    try out.interface.print("vopr: {d} seed(s) x {d} steps passed (0 failures, 0 leaks)\n", .{ end_seed - start_seed, steps });
    try out.interface.flush();
    return 0;
}
