const std = @import("std");
const schema = @import("../api/schema.zig");

const UnionMeta = struct {
    union_name: []const u8,
    variant_name: []const u8,
};

fn find_union_meta(comptime Rec: type) ?UnionMeta {
    @setEvalBranchQuota(100_000);
    inline for (schema.wire_unions) |U| {
        const info = @typeInfo(U).@"union";
        inline for (info.field_names, info.field_types) |fname, ftype| {
            if (ftype == Rec) {
                return .{
                    .union_name = comptime simple_name(@typeName(U)),
                    .variant_name = fname,
                };
            }
        }
    }
    return null;
}

fn simple_name(full: []const u8) []const u8 {
    @setEvalBranchQuota(50_000);
    if (std.mem.findScalarLast(u8, full, '.')) |dot| {
        return full[dot + 1 ..];
    }
    return full;
}

fn strip_ms_suffix(name: []const u8) []const u8 {
    if (std.mem.endsWith(u8, name, "_ms")) {
        return name[0 .. name.len - 3];
    }
    return name;
}

fn snake_to_camel(gpa: std.mem.Allocator, snake: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    var upper = false;
    for (snake, 0..) |c, idx| {
        if (c == '_') {
            upper = true;
            continue;
        }
        if (upper and out.items.len > 0) {
            try out.append(gpa, std.ascii.toUpper(c));
        } else if (idx == 0) {
            try out.append(gpa, std.ascii.toLower(c));
        } else {
            try out.append(gpa, c);
        }
        upper = false;
    }
    return out.toOwnedSlice(gpa);
}

fn snake_to_pascal(gpa: std.mem.Allocator, snake: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(gpa);
    var upper = true;
    for (snake) |c| {
        if (c == '_') {
            upper = true;
            continue;
        }
        if (upper) {
            try out.append(gpa, std.ascii.toUpper(c));
            upper = false;
        } else {
            try out.append(gpa, c);
        }
    }
    return out.toOwnedSlice(gpa);
}

fn is_enum_type(comptime T: type) bool {
    return switch (@typeInfo(T)) {
        .@"enum" => true,
        else => false,
    };
}

fn is_union_type(comptime T: type) bool {
    return switch (@typeInfo(T)) {
        .@"union" => true,
        else => false,
    };
}

fn dart_type_name(comptime T: type, gpa: std.mem.Allocator) ![]const u8 {
    if (T == schema.TimeMs) return "DateTime";
    if (T == schema.JsonValue) return "Object?";
    if (T == []const u8) return "String";
    if (T == bool) return "bool";
    if (T == f64 or T == f32) return "double";
    if (T == u64 or T == i64 or T == u32 or T == i32 or T == usize) return "int";

    switch (@typeInfo(T)) {
        .optional => |opt| {
            const inner = try dart_type_name(opt.child, gpa);
            if (std.mem.endsWith(u8, inner, "?")) return inner;
            return std.fmt.allocPrint(gpa, "{s}?", .{inner});
        },
        .pointer => |ptr| {
            if (ptr.size == .slice) {
                const elem = try dart_type_name(ptr.child, gpa);
                return std.fmt.allocPrint(gpa, "List<{s}>", .{elem});
            }
            @compileError("unsupported pointer type in schema: " ++ @typeName(T));
        },
        .@"enum", .@"struct", .@"union" => return simple_name(@typeName(T)),
        else => @compileError("unsupported field type in schema: " ++ @typeName(T)),
    }
}

fn has_non_null_default(comptime T: type, comptime attrs: anytype) bool {
    if (attrs.defaultValue(T)) |d| {
        if (@typeInfo(T) == .optional) {
            return d != null;
        }
        return true;
    }
    return false;
}

fn write_parse_expr(
    comptime T: type,
    gpa: std.mem.Allocator,
    w: *std.Io.Writer,
    expr: []const u8,
    ctx_name: []const u8,
) !void {
    if (T == schema.TimeMs) {
        try w.print("_time({s}, '{s}')", .{ expr, ctx_name });
        return;
    }
    if (T == schema.JsonValue) {
        try w.writeAll(expr);
        return;
    }
    if (T == []const u8) {
        try w.print("_string({s}, '{s}')", .{ expr, ctx_name });
        return;
    }
    if (T == bool) {
        try w.print("_bool({s}, '{s}')", .{ expr, ctx_name });
        return;
    }
    if (T == f64 or T == f32) {
        try w.print("_double({s}, '{s}')", .{ expr, ctx_name });
        return;
    }
    if (T == u64 or T == i64 or T == u32 or T == i32 or T == usize) {
        try w.print("_int({s}, '{s}')", .{ expr, ctx_name });
        return;
    }

    switch (@typeInfo(T)) {
        .optional => |opt| {
            if (opt.child == schema.TimeMs or
                opt.child == []const u8 or
                opt.child == bool or
                opt.child == f64 or
                opt.child == f32 or
                opt.child == u64 or
                opt.child == i64 or
                opt.child == u32 or
                opt.child == i32 or
                opt.child == usize)
            {
                try w.print("_opt({s}, (e0) => ", .{expr});
                try write_parse_expr(opt.child, gpa, w, "e0", ctx_name);
                try w.writeAll(")");
            } else if (@typeInfo(opt.child) == .pointer) {
                try w.print("_opt({s}, (e0) => ", .{expr});
                try write_parse_expr(opt.child, gpa, w, "e0", ctx_name);
                try w.writeAll(")");
            } else {
                const tn = simple_name(@typeName(opt.child));
                try w.print("_opt({s}, {s}.fromJson)", .{ expr, tn });
            }
        },
        .pointer => |ptr| {
            if (ptr.size == .slice) {
                try w.print("_list({s}, '{s}', ", .{ expr, ctx_name });
                if (is_enum_type(ptr.child) or @typeInfo(ptr.child) == .@"struct" or is_union_type(ptr.child)) {
                    if (ptr.child == schema.TimeMs) {
                        try w.print("(e0) => _time(e0, '{s}')", .{ctx_name});
                    } else {
                        try w.print("{s}.fromJson", .{simple_name(@typeName(ptr.child))});
                    }
                } else {
                    try w.writeAll("(e0) => ");
                    try write_parse_expr(ptr.child, gpa, w, "e0", ctx_name);
                }
                try w.writeAll(")");
            }
        },
        .@"enum", .@"struct", .@"union" => {
            try w.print("{s}.fromJson({s})", .{ simple_name(@typeName(T)), expr });
        },
        else => unreachable,
    }
}

fn write_to_json_expr(comptime T: type, w: *std.Io.Writer, expr: []const u8) !void {
    if (T == schema.TimeMs) {
        try w.print("{s}.millisecondsSinceEpoch", .{expr});
        return;
    }
    if (T == schema.JsonValue or
        T == []const u8 or
        T == bool or
        T == f64 or
        T == f32 or
        T == u64 or
        T == i64 or
        T == u32 or
        T == i32 or
        T == usize)
    {
        try w.writeAll(expr);
        return;
    }
    switch (@typeInfo(T)) {
        .optional => |opt| {
            try write_to_json_expr(opt.child, w, expr);
        },
        .pointer => |ptr| {
            if (ptr.size == .slice) {
                if (ptr.child == []const u8 or ptr.child == bool or ptr.child == f64 or ptr.child == u64 or ptr.child == i64) {
                    try w.writeAll(expr);
                } else if (ptr.child == schema.TimeMs) {
                    try w.print("[for (final e in {s}) e.millisecondsSinceEpoch]", .{expr});
                } else {
                    try w.print("[for (final e in {s}) e.toJson()]", .{expr});
                }
            }
        },
        .@"enum", .@"struct", .@"union" => {
            try w.print("{s}.toJson()", .{expr});
        },
        else => unreachable,
    }
}

fn write_enum(comptime E: type, gpa: std.mem.Allocator, w: *std.Io.Writer) !void {
    const name = simple_name(@typeName(E));
    const info = @typeInfo(E).@"enum";
    try w.print("enum {s} {{\n", .{name});
    for (info.field_names, 0..) |fname, idx| {
        const camel = try snake_to_camel(gpa, fname);
        const sep: []const u8 = if (idx + 1 < info.field_names.len) "," else ";";
        try w.print("  {s}('{s}'){s}\n", .{ camel, fname, sep });
    }
    try w.print(
        \\
        \\  const {s}(this.wire);
        \\
        \\  final String wire;
        \\
        \\  static {s} fromJson(Object? json) {{
        \\    for (final v in values) {{
        \\      if (v.wire == json) return v;
        \\    }}
        \\    return unknown;
        \\  }}
        \\
        \\  String toJson() => wire;
        \\}}
        \\
        \\
    , .{ name, name });
}

fn write_sealed_union(comptime U: type, gpa: std.mem.Allocator, w: *std.Io.Writer) !void {
    const uname = comptime simple_name(@typeName(U));
    const info = @typeInfo(U).@"union";

    try w.print("sealed class {s} {{\n", .{uname});
    try w.print("  const {s}();\n\n", .{uname});
    if (comptime std.mem.eql(u8, uname, "HubStreamMessage")) {
        try w.writeAll("  int get seq;\n\n");
    }
    try w.print("  factory {s}.fromJson(Object? json) {{\n", .{uname});
    try w.print("    final j = _object(json, '{s}');\n", .{uname});
    try w.writeAll("    return switch (j['type']) {\n");

    inline for (info.field_names, info.field_types) |fname, ftype| {
        if (comptime std.mem.eql(u8, fname, "unknown")) continue;
        const arm_name = comptime simple_name(@typeName(ftype));
        try w.print("      '{s}' => {s}._fromMap(j),\n", .{ fname, arm_name });
    }

    try w.print("      _ => {s}Unknown(j),\n", .{uname});
    try w.writeAll("    };\n  }\n\n");
    try w.writeAll("  Map<String, Object?> toJson();\n}\n\n");

    inline for (info.field_names, info.field_types) |fname, ftype| {
        if (comptime std.mem.eql(u8, fname, "unknown")) continue;
        const umeta = comptime UnionMeta{
            .union_name = uname,
            .variant_name = fname,
        };
        try write_record(ftype, umeta, gpa, w);
    }

    if (comptime std.mem.eql(u8, uname, "HubStreamMessage")) {
        try w.print(
            \\final class {s}Unknown extends {s} {{
            \\  const {s}Unknown(this.json);
            \\
            \\  final Map<String, Object?> json;
            \\
            \\  @override
            \\  int get seq => switch (json['seq']) {{
            \\    final int n => n,
            \\    final num n => n.toInt(),
            \\    _ => 0,
            \\  }};
            \\
            \\  @override
            \\  Map<String, Object?> toJson() => json;
            \\}}
            \\
            \\
        , .{ uname, uname, uname });
    } else {
        try w.print(
            \\final class {s}Unknown extends {s} {{
            \\  const {s}Unknown(this.json);
            \\
            \\  final Map<String, Object?> json;
            \\
            \\  @override
            \\  Map<String, Object?> toJson() => json;
            \\}}
            \\
            \\
        , .{ uname, uname, uname });
    }
}

fn write_record(
    comptime R: type,
    comptime umeta: ?UnionMeta,
    gpa: std.mem.Allocator,
    w: *std.Io.Writer,
) !void {
    const rname = simple_name(@typeName(R));
    const info = @typeInfo(R).@"struct";

    if (umeta) |m| {
        try w.print("final class {s} extends {s} {{\n", .{ rname, m.union_name });
    } else {
        try w.print("final class {s} {{\n", .{rname});
    }

    if (info.field_names.len == 0) {
        try w.print("  const {s}();\n\n", .{rname});
        try w.print("  factory {s}.fromJson(Object? json) {{\n", .{rname});
        try w.print("    _object(json, '{s}');\n", .{rname});
        try w.print("    return const {s}();\n  }}\n\n", .{rname});
        try w.writeAll("  Map<String, Object?> toJson() => const {};\n}\n\n");
        return;
    }

    try w.print("  const {s}({{\n", .{rname});
    inline for (info.field_names, info.field_types, info.field_attrs) |fname, ftype, fattr| {
        const base_name = if (ftype == schema.TimeMs or ftype == ?schema.TimeMs) strip_ms_suffix(fname) else fname;
        const dart_field = try snake_to_camel(gpa, base_name);

        if (ftype == schema.JsonValue) {
            try w.print("    this.{s},\n", .{dart_field});
        } else if (@typeInfo(ftype) == .optional) {
            try w.print("    this.{s},\n", .{dart_field});
        } else if (comptime has_non_null_default(ftype, fattr)) {
            if (ftype == bool) {
                const defv = fattr.defaultValue(bool).?;
                try w.print("    this.{s} = {s},\n", .{ dart_field, if (defv) "true" else "false" });
            } else if (ftype == []const u8) {
                try w.print("    this.{s} = '',\n", .{dart_field});
            } else if (@typeInfo(ftype) == .pointer) {
                const elem_dart = try dart_type_name(@typeInfo(ftype).pointer.child, gpa);
                try w.print("    this.{s} = const <{s}>[],\n", .{ dart_field, elem_dart });
            } else if (ftype == u64 or ftype == i64 or ftype == u32 or ftype == i32 or ftype == usize) {
                const defv = fattr.defaultValue(ftype).?;
                try w.print("    this.{s} = {d},\n", .{ dart_field, defv });
            } else if (@typeInfo(ftype) == .@"enum") {
                const defv = fattr.defaultValue(ftype).?;
                const enum_name = simple_name(@typeName(ftype));
                const var_camel = try snake_to_camel(gpa, @tagName(defv));
                try w.print("    this.{s} = {s}.{s},\n", .{ dart_field, enum_name, var_camel });
            } else if (ftype == schema.Usage) {
                try w.print("    this.{s} = const Usage(),\n", .{dart_field});
            } else {
                try w.print("    required this.{s},\n", .{dart_field});
            }
        } else {
            try w.print("    required this.{s},\n", .{dart_field});
        }
    }
    try w.writeAll("  });\n\n");

    if (umeta != null) {
        try w.print("  factory {s}.fromJson(Object? json) => {s}._fromMap(_object(json, '{s}'));\n\n", .{ rname, rname, rname });
        try w.print("  factory {s}._fromMap(Map<String, Object?> j) => {s}(\n", .{ rname, rname });
    } else {
        try w.print("  factory {s}.fromJson(Object? json) {{\n", .{rname});
        try w.print("    final j = _object(json, '{s}');\n", .{rname});
        try w.print("    return {s}(\n", .{rname});
    }

    inline for (info.field_names, info.field_types, info.field_attrs) |fname, ftype, fattr| {
        const base_name = if (ftype == schema.TimeMs or ftype == ?schema.TimeMs) strip_ms_suffix(fname) else fname;
        const dart_field = try snake_to_camel(gpa, base_name);
        const ctx = try std.fmt.allocPrint(gpa, "{s}.{s}", .{ rname, fname });
        const key_expr = try std.fmt.allocPrint(gpa, "j['{s}']", .{fname});

        try w.print("      {s}: ", .{dart_field});
        if (ftype == schema.JsonValue) {
            try w.print("{s},\n", .{key_expr});
        } else if (comptime has_non_null_default(ftype, fattr) and @typeInfo(ftype) != .optional and ftype != schema.JsonValue) {
            if (ftype == bool) {
                const defv = fattr.defaultValue(bool).?;
                try w.print("_or({s}, {s}, (e0) => _bool(e0, '{s}')),\n", .{ key_expr, if (defv) "true" else "false", ctx });
            } else if (ftype == []const u8) {
                try w.print("_or({s}, '', (e0) => _string(e0, '{s}')),\n", .{ key_expr, ctx });
            } else if (@typeInfo(ftype) == .pointer) {
                const elem_dart = try dart_type_name(@typeInfo(ftype).pointer.child, gpa);
                try w.print("_or({s}, const <{s}>[], (e0) => ", .{ key_expr, elem_dart });
                try write_parse_expr(ftype, gpa, w, "e0", ctx);
                try w.writeAll("),\n");
            } else if (ftype == u64 or ftype == i64 or ftype == u32 or ftype == i32 or ftype == usize) {
                const defv = fattr.defaultValue(ftype).?;
                try w.print("_or({s}, {d}, (e0) => _int(e0, '{s}')),\n", .{ key_expr, defv, ctx });
            } else if (@typeInfo(ftype) == .@"enum") {
                const defv = fattr.defaultValue(ftype).?;
                const enum_name = simple_name(@typeName(ftype));
                const var_camel = try snake_to_camel(gpa, @tagName(defv));
                try w.print("{s} == null ? {s}.{s} : {s}.fromJson({s}),\n", .{ key_expr, enum_name, var_camel, enum_name, key_expr });
            } else if (ftype == schema.Usage) {
                try w.print("{s} == null ? const Usage() : Usage.fromJson({s}),\n", .{ key_expr, key_expr });
            } else {
                try write_parse_expr(ftype, gpa, w, key_expr, ctx);
                try w.writeAll(",\n");
            }
        } else {
            try write_parse_expr(ftype, gpa, w, key_expr, ctx);
            try w.writeAll(",\n");
        }
    }

    if (umeta != null) {
        try w.writeAll("  );\n\n");
    } else {
        try w.writeAll("    );\n  }\n\n");
    }

    inline for (info.field_names, info.field_types) |fname, ftype| {
        const base_name = if (ftype == schema.TimeMs or ftype == ?schema.TimeMs) strip_ms_suffix(fname) else fname;
        const dart_field = try snake_to_camel(gpa, base_name);
        const dt = try dart_type_name(ftype, gpa);
        if (umeta) |m| {
            if (comptime std.mem.eql(u8, m.union_name, "HubStreamMessage") and std.mem.eql(u8, fname, "seq")) {
                try w.writeAll("  @override\n");
            }
        }
        try w.print("  final {s} {s};\n\n", .{ dt, dart_field });
    }

    if (umeta != null) try w.writeAll("  @override\n");
    try w.writeAll("  Map<String, Object?> toJson() => {\n");
    if (umeta) |m| {
        try w.print("    'type': '{s}',\n", .{m.variant_name});
    }
    inline for (info.field_names, info.field_types) |fname, ftype| {
        const base_name = if (ftype == schema.TimeMs or ftype == ?schema.TimeMs) strip_ms_suffix(fname) else fname;
        const dart_field = try snake_to_camel(gpa, base_name);
        if (@typeInfo(ftype) == .optional or ftype == schema.JsonValue) {
            try w.print("    if ({s} case final v?) '{s}': ", .{ dart_field, fname });
            try write_to_json_expr(ftype, w, "v");
            try w.writeAll(",\n");
        } else {
            try w.print("    '{s}': ", .{fname});
            try write_to_json_expr(ftype, w, dart_field);
            try w.writeAll(",\n");
        }
    }
    try w.writeAll("  };\n}\n\n");
}

fn generate_dart(gpa: std.mem.Allocator) ![]u8 {
    var aw: std.Io.Writer.Allocating = .init(gpa);
    defer aw.deinit();
    const w = &aw.writer;

    try w.writeAll(
        \\// Generated by protectorate zig apigen from src/api/schema.zig. Do not edit.
        \\// ignore_for_file: lines_longer_than_80_chars
        \\
        \\import 'dart:async';
        \\import 'dart:convert';
        \\
        \\import 'transport.dart';
        \\
        \\Never _bad(String what, Object? json) =>
        \\    throw FormatException('Expected $what, got ${json.runtimeType}');
        \\
        \\Map<String, Object?> _object(Object? json, String what) =>
        \\    json is Map<String, Object?> ? json : _bad(what, json);
        \\
        \\bool _bool(Object? json, String what) =>
        \\    json is bool ? json : _bad(what, json);
        \\
        \\int _int(Object? json, String what) =>
        \\    json is int ? json : (json is num ? json.toInt() : _bad(what, json));
        \\
        \\double _double(Object? json, String what) =>
        \\    json is num ? json.toDouble() : _bad(what, json);
        \\
        \\String _string(Object? json, String what) =>
        \\    json is String ? json : _bad(what, json);
        \\
        \\DateTime _time(Object? json, String what) =>
        \\    DateTime.fromMillisecondsSinceEpoch(_int(json, what), isUtc: true);
        \\
        \\List<T> _list<T>(Object? json, String what, T Function(Object?) item) =>
        \\    json is List ? [for (final e in json) item(e)] : _bad(what, json);
        \\
        \\T? _opt<T>(Object? json, T Function(Object?) some) =>
        \\    json == null ? null : some(json);
        \\
        \\T _or<T>(Object? json, T fallback, T Function(Object?) parse) =>
        \\    json == null ? fallback : parse(json);
        \\
        \\
    );

    inline for (schema.wire_enums) |E| {
        try write_enum(E, gpa, w);
    }

    inline for (schema.wire_unions) |U| {
        try write_sealed_union(U, gpa, w);
    }

    inline for (schema.wire_records) |R| {
        if (comptime find_union_meta(R) == null) {
            try write_record(R, null, gpa, w);
        }
    }

    try w.writeAll(
        \\final class ApiException implements Exception {
        \\  const ApiException(this.status, this.code, this.message);
        \\
        \\  final int status;
        \\  final ErrorCode code;
        \\  final String message;
        \\
        \\  @override
        \\  String toString() => '${code.wire}: $message';
        \\}
        \\
        \\ApiException _error(int status, String body) {
        \\  try {
        \\    final e = ErrorBody.fromJson(jsonDecode(body)).error;
        \\    return ApiException(status, e.code, e.message);
        \\  } on FormatException {
        \\    final text = body.length > 200 ? '${body.substring(0, 200)}...' : body;
        \\    return ApiException(status, ErrorCode.unknown, 'HTTP $status $text');
        \\  }
        \\}
        \\
        \\final class HubApi {
        \\  HubApi(this.baseUrl, {this.token});
        \\
        \\  final Uri baseUrl;
        \\  final String? token;
        \\
        \\  Uri _url(String path, [Map<String, String>? query]) {
        \\    final base = baseUrl.path.endsWith('/')
        \\        ? baseUrl.path.substring(0, baseUrl.path.length - 1)
        \\        : baseUrl.path;
        \\    return baseUrl.replace(path: '$base$path', queryParameters: query);
        \\  }
        \\
        \\  Future<T> _call<T>(
        \\    String method,
        \\    Map<String, Object?> request,
        \\    T Function(Object?) parse,
        \\  ) async {
        \\    final r = await postJson(_url('/api/$method'), jsonEncode(request), token);
        \\    if (r.status < 200 || r.status > 299) throw _error(r.status, r.body);
        \\    return parse(jsonDecode(r.body));
        \\  }
        \\
        \\  Stream<HubStreamMessage> events({
        \\    int? fromSeq,
        \\    Iterable<String> transcripts = const [],
        \\  }) => openSocket(
        \\    _url('/api/events', {
        \\      if (fromSeq != null) 'from_seq': '$fromSeq',
        \\      if (transcripts.isNotEmpty) 'transcripts': transcripts.join(','),
        \\    }),
        \\    token,
        \\  ).map((text) => HubStreamMessage.fromJson(jsonDecode(text)));
        \\
        \\  Uri clientUrl(String name) => _url('/api/client/$name');
        \\
        \\
    );

    inline for (schema.methods) |m| {
        const req_name = simple_name(@typeName(m.Req));
        const resp_name = simple_name(@typeName(m.Resp));
        if (m.default_empty and m.Req == schema.Empty) {
            try w.print(
                \\  /// `POST /api/{s}`, scope `{s}`.
                \\  Future<{s}> {s}([Empty request = const Empty()]) =>
                \\      _call('{s}', request.toJson(), {s}.fromJson);
                \\
                \\
            , .{ m.name, m.scope.to_wire(), resp_name, m.dart_name, m.name, resp_name });
        } else {
            try w.print(
                \\  /// `POST /api/{s}`, scope `{s}`.
                \\  Future<{s}> {s}({s} request) =>
                \\      _call('{s}', request.toJson(), {s}.fromJson);
                \\
                \\
            , .{ m.name, m.scope.to_wire(), resp_name, m.dart_name, req_name, m.name, resp_name });
        }
    }

    try w.writeAll("}\n");
    return gpa.dupe(u8, aw.written());
}

pub fn main(init: std.process.Init) !u8 {
    var arena_state = std.heap.ArenaAllocator.init(init.gpa);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const args = try init.minimal.args.toSlice(arena);
    var out_path: ?[]const u8 = null;
    var check_path: ?[]const u8 = null;

    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        if (std.mem.eql(u8, args[i], "--out") and i + 1 < args.len) {
            i += 1;
            out_path = args[i];
        } else if (std.mem.eql(u8, args[i], "--check") and i + 1 < args.len) {
            i += 1;
            check_path = args[i];
        }
    }

    const generated = try generate_dart(arena);

    if (check_path) |cp| {
        const existing = std.Io.Dir.cwd().readFileAlloc(init.io, cp, arena, .limited(4 * 1024 * 1024)) catch {
            std.debug.print("apigen: cannot read {s}\n", .{cp});
            return 1;
        };
        if (!std.mem.eql(u8, existing, generated)) {
            std.debug.print("apigen: {s} is out of date; run `zig build gen`\n", .{cp});
            return 1;
        }
        return 0;
    }

    if (out_path) |op| {
        if (std.mem.findScalarLast(u8, op, '/')) |slash| {
            try std.Io.Dir.cwd().createDirPath(init.io, op[0..slash]);
        }
        try std.Io.Dir.cwd().writeFile(init.io, .{
            .sub_path = op,
            .data = generated,
        });
        return 0;
    }

    var buf: [4096]u8 = undefined;
    var stdout = std.Io.File.stdout().writer(init.io, &buf);
    try stdout.interface.writeAll(generated);
    try stdout.interface.flush();
    return 0;
}
