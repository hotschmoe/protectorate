//
//  Generated code. Do not modify.
//  source: protectorate/v1/desk.proto
//

import "package:connectrpc/connect.dart" as connect;
import "desk.pb.dart" as protectoratev1desk;

/// DeskService drives the interactive desktop of a node, for programs that
/// only have a GUI. It works only when the node agent runs in the logged-in
/// user's session (`protectorate service windows-task`); otherwise calls fail
/// with FAILED_PRECONDITION. Only Windows nodes implement it.
/// The desktop is a lease resource named "desk:<node>". Every command that
/// changes the desktop (click, move, type, key, run) must name a live lease on
/// it; screenshot and windows do not. Coordinates are pixels of the
/// screenshot, whose origin is the top-left of the virtual screen.
abstract final class DeskService {
  /// Fully-qualified name of the DeskService service.
  static const name = 'protectorate.v1.DeskService';

  /// Screenshot captures the whole virtual screen as a PNG.
  static const screenshot = connect.Spec(
    '/$name/Screenshot',
    connect.StreamType.unary,
    protectoratev1desk.ScreenshotRequest.new,
    protectoratev1desk.ScreenshotResponse.new,
  );

  /// ListWindows lists visible top-level windows, topmost first.
  static const listWindows = connect.Spec(
    '/$name/ListWindows',
    connect.StreamType.unary,
    protectoratev1desk.ListWindowsRequest.new,
    protectoratev1desk.ListWindowsResponse.new,
  );

  /// Click moves the pointer and clicks. Needs a desk lease.
  static const click = connect.Spec(
    '/$name/Click',
    connect.StreamType.unary,
    protectoratev1desk.ClickRequest.new,
    protectoratev1desk.InputResponse.new,
  );

  /// Move puts the pointer at a position. Needs a desk lease.
  static const move = connect.Spec(
    '/$name/Move',
    connect.StreamType.unary,
    protectoratev1desk.MoveRequest.new,
    protectoratev1desk.InputResponse.new,
  );

  /// Type enters text as Unicode key events into the focused window. Needs a desk
  /// lease.
  static const type = connect.Spec(
    '/$name/Type',
    connect.StreamType.unary,
    protectoratev1desk.TypeRequest.new,
    protectoratev1desk.InputResponse.new,
  );

  /// Key presses a chord such as "ctrl+s", "alt+f4" or "enter". Needs a desk
  /// lease.
  static const key = connect.Spec(
    '/$name/Key',
    connect.StreamType.unary,
    protectoratev1desk.KeyRequest.new,
    protectoratev1desk.InputResponse.new,
  );

  /// Run starts a program in the interactive session and returns at once. Needs
  /// a desk lease.
  static const run = connect.Spec(
    '/$name/Run',
    connect.StreamType.unary,
    protectoratev1desk.RunRequest.new,
    protectoratev1desk.RunResponse.new,
  );
}
