//
//  Generated code. Do not modify.
//  source: protectorate/v1/desk.proto
//

import "package:connectrpc/connect.dart" as connect;
import "desk.pb.dart" as protectoratev1desk;
import "desk.connect.spec.dart" as specs;

/// DeskService drives the interactive desktop of a node, for programs that
/// only have a GUI. It works only when the node agent runs in the logged-in
/// user's session (`protectorate node install`); otherwise calls fail
/// with FAILED_PRECONDITION. Only Windows nodes implement it.
/// The desktop is a lease resource named "desk:<node>". Every command that
/// changes the desktop (click, move, type, key, run) runs under a live lease on
/// it; screenshot and windows need none. A command that names no lease makes
/// the hub take a 2 minute lease for the caller when the desktop is free (the
/// id and expiry come back in the Protectorate-Desk-Lease and
/// Protectorate-Desk-Lease-Expires response headers), renew the caller's own,
/// or refuse when another holder has it. Coordinates are pixels of the
/// screenshot, whose origin is the top-left of the virtual screen.
/// The desktop is the last resort. Prefer, in this order: the program's own API
/// or batch/CLI mode; `protectorate job run` or ssh on the node; `protectorate
/// node put/get` for files; only then screen and input, which are slow, flaky
/// and can land in the wrong window.
extension type DeskServiceClient (connect.Transport _transport) {
  /// Screenshot captures the whole virtual screen as a PNG.
  Future<protectoratev1desk.ScreenshotResponse> screenshot(
    protectoratev1desk.ScreenshotRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.screenshot,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListWindows lists visible top-level windows, topmost first.
  Future<protectoratev1desk.ListWindowsResponse> listWindows(
    protectoratev1desk.ListWindowsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.listWindows,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Foreground reports the window that receives keystrokes now, with its
  /// process and integrity level. Read-only. Windows drops input injected into
  /// a window of a higher integrity level than the agent's without any error,
  /// so Click, Move, Type and Key refuse with FAILED_PRECONDITION when the
  /// foreground window (or, for the mouse, the window under the pointer) is
  /// higher; this shows what is in the way.
  Future<protectoratev1desk.ForegroundResponse> foreground(
    protectoratev1desk.ForegroundRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.foreground,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Click moves the pointer and clicks.
  /// Last resort: prefer the shell (job run, ssh), node put/get, or the
  /// program's API. Needs a desk lease.
  Future<protectoratev1desk.InputResponse> click(
    protectoratev1desk.ClickRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.click,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Move puts the pointer at a position.
  /// Last resort: prefer the shell (job run, ssh), node put/get, or the
  /// program's API. Needs a desk lease.
  Future<protectoratev1desk.InputResponse> move(
    protectoratev1desk.MoveRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.move,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Type enters text as key presses into the focused window. Last resort:
  /// prefer the shell (job run, ssh), node put/get, or the program's API. Needs
  /// a desk lease.
  Future<protectoratev1desk.InputResponse> type(
    protectoratev1desk.TypeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.type,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Key presses a chord such as "ctrl+s", "alt+f4" or "enter". Last resort:
  /// prefer the shell (job run, ssh), node put/get, or the program's API. Needs
  /// a desk lease.
  Future<protectoratev1desk.InputResponse> key(
    protectoratev1desk.KeyRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.key,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Run starts a program in the interactive session and returns at once. Last
  /// resort: prefer the shell (job run, ssh), node put/get, or the program's
  /// API. Needs a desk lease.
  Future<protectoratev1desk.RunResponse> run(
    protectoratev1desk.RunRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeskService.run,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
