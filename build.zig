const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const ver = b.option([]const u8, "version", "Release version string") orelse "0.0.0-dev";
    const git_commit = b.option([]const u8, "git-commit", "Git commit hash") orelse "unknown";
    const web_dir = b.option([]const u8, "web-dir", "Flutter web assets directory") orelse "app/build/web";
    const strip_opt = b.option(bool, "strip", "Strip debug symbols from installed binaries");

    const build_options = b.addOptions();
    build_options.addOption([]const u8, "version", ver);
    build_options.addOption([]const u8, "git_commit", git_commit);

    const host_target = b.graph.host;

    const levels_exe = b.addExecutable(.{
        .name = "pt-levels",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main_levels.zig"),
            .target = host_target,
            .optimize = .safe,
        }),
    });

    const apigen_exe = b.addExecutable(.{
        .name = "pt-apigen",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main_apigen.zig"),
            .target = host_target,
            .optimize = .safe,
        }),
    });

    const embed_web_exe = b.addExecutable(.{
        .name = "pt-embed-web",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main_embed_web.zig"),
            .target = host_target,
            .optimize = .safe,
        }),
    });

    const pack_web = b.addRunArtifact(embed_web_exe);
    pack_web.addArg("--in");
    pack_web.addArg(web_dir);
    pack_web.addArg("--out");
    const web_out_dir = pack_web.addOutputDirectoryArg("web_assets");
    const web_assets_mod = b.createModule(.{
        .root_source_file = web_out_dir.path(b, "assets.zig"),
    });

    const hub_mod = b.createModule(.{
        .root_source_file = b.path("src/main_hub.zig"),
        .target = target,
        .optimize = optimize,
        .strip = strip_opt,
    });
    hub_mod.addOptions("build_options", build_options);
    hub_mod.addImport("web_assets", web_assets_mod);

    const hub_exe = b.addExecutable(.{
        .name = "protectorate",
        .root_module = hub_mod,
    });
    b.installArtifact(hub_exe);

    const node_mod = b.createModule(.{
        .root_source_file = b.path("src/main_node.zig"),
        .target = target,
        .optimize = optimize,
        .strip = strip_opt,
    });
    node_mod.addOptions("build_options", build_options);

    const node_exe = b.addExecutable(.{
        .name = "protectorate-node",
        .root_module = node_mod,
    });
    b.installArtifact(node_exe);

    const check_levels_run = b.addRunArtifact(levels_exe);
    check_levels_run.addArg("--root");
    check_levels_run.addArg("src");
    check_levels_run.addPassthruArgs();
    check_levels_run.has_side_effects = true;
    const check_levels_step = b.step("check-levels", "Verify module DAG and layer rules");
    check_levels_step.dependOn(&check_levels_run.step);

    const gen_run = b.addRunArtifact(apigen_exe);
    gen_run.addArg("--out");
    gen_run.addArg("app/lib/api/api.g.dart");
    gen_run.has_side_effects = true;
    const gen_step = b.step("gen", "Generate Dart API client from src/api/schema.zig");
    gen_step.dependOn(&gen_run.step);

    const check_gen_run = b.addRunArtifact(apigen_exe);
    check_gen_run.addArg("--check");
    check_gen_run.addArg("app/lib/api/api.g.dart");
    check_gen_run.has_side_effects = true;
    const check_gen_step = b.step("check-gen", "Verify generated Dart API matches schema");
    check_gen_step.dependOn(&check_gen_run.step);

    const vopr_mod = b.createModule(.{
        .root_source_file = b.path("src/main_vopr.zig"),
        .target = host_target,
        .optimize = optimize,
    });
    const vopr_exe = b.addExecutable(.{
        .name = "vopr",
        .root_module = vopr_mod,
    });
    const dst_run = b.addRunArtifact(vopr_exe);
    dst_run.addPassthruArgs();
    dst_run.has_side_effects = true;
    const dst_step = b.step("dst", "Run deterministic simulation testing (VOPR)");
    dst_step.dependOn(&dst_run.step);

    const check_step = b.step("check", "Run level check, codegen check, DST, and install build");
    check_step.dependOn(check_levels_step);
    check_step.dependOn(check_gen_step);
    check_step.dependOn(dst_step);
    check_step.dependOn(b.getInstallStep());

    const release_step = b.step("release-all", "Cross-compile Linux, Windows, and macOS release binaries");
    const release_specs = [_]struct { label: []const u8, query: []const u8 }{
        .{ .label = "x86_64-linux-musl", .query = "x86_64-linux-musl" },
        .{ .label = "aarch64-linux-musl", .query = "aarch64-linux-musl" },
        .{ .label = "x86_64-windows-gnu", .query = "x86_64-windows-gnu" },
        .{ .label = "aarch64-windows-gnu", .query = "aarch64-windows-gnu" },
        .{ .label = "aarch64-macos", .query = "aarch64-macos" },
    };

    for (release_specs) |spec| {
        const parsed_q = std.Target.Query.parse(.{ .arch_os_abi = spec.query }) catch continue;
        const r_target = b.resolveTargetQuery(parsed_q);

        const r_web = b.createModule(.{
            .root_source_file = web_out_dir.path(b, "assets.zig"),
        });

        const r_hub_mod = b.createModule(.{
            .root_source_file = b.path("src/main_hub.zig"),
            .target = r_target,
            .optimize = .safe,
            .strip = true,
        });
        r_hub_mod.addOptions("build_options", build_options);
        r_hub_mod.addImport("web_assets", r_web);
        const r_hub = b.addExecutable(.{
            .name = "protectorate",
            .root_module = r_hub_mod,
        });
        const inst_hub = b.addInstallArtifact(r_hub, .{
            .dest_dir = .{ .override = .{ .custom = b.fmt("release/{s}", .{spec.label}) } },
        });
        release_step.dependOn(&inst_hub.step);

        const r_node_mod = b.createModule(.{
            .root_source_file = b.path("src/main_node.zig"),
            .target = r_target,
            .optimize = .safe,
            .strip = true,
        });
        r_node_mod.addOptions("build_options", build_options);
        const r_node = b.addExecutable(.{
            .name = "protectorate-node",
            .root_module = r_node_mod,
        });
        const inst_node = b.addInstallArtifact(r_node, .{
            .dest_dir = .{ .override = .{ .custom = b.fmt("release/{s}", .{spec.label}) } },
        });
        release_step.dependOn(&inst_node.step);
    }
}
