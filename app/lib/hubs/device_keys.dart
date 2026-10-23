import 'device_key.dart';
import 'device_keys_io.dart'
    if (dart.library.js_interop) 'device_keys_web.dart'
    as platform;

/// Each hub profile has its own device key, created when it pairs.
abstract final class DeviceKeys {
  static Future<DeviceKey?> load(String profile) => platform.load(profile);

  static Future<DeviceKey> create(String profile) => platform.create(profile);

  static Future<void> forget(String profile) => platform.forget(profile);
}
