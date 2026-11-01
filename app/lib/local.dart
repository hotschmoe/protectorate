// What the app keeps on the device, per platform: the browser's localStorage and location on
// web; in memory elsewhere (REVISIT: a settings file through dart:io).
//
//   String? loadSetting(String key)
//   void saveSetting(String key, String? value)   // null removes it
//   String? pageOrigin()                          // the page's http(s) origin on web, else null
//   void dropQueryParam(String key)               // rewrites the address bar without it
export 'local_web.dart' if (dart.library.io) 'local_io.dart';
