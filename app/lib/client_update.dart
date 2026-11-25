// The app updating itself to the hub's release, per platform: Android and Windows download
// their build from the hub and install it; on web the hub serves the app, so an update is a
// reload.
//
//   const bool reloadsFromHub                     // true on web
//   String? clientAsset()                         // this build's name in HubInfo.clients, or null
//   void reloadPage()                             // web only
//   Future<String> downloadClient(Uri url, String? token,
//       void Function(int got, int total) progress)   // the downloaded file's path
//   String? installHint()                         // what to tell the owner before installing, or null
//   Future<String> installClient(String path, String version)  // what happens next; Windows exits instead
//   Future<({String text, bool ok})?> lastInstall(String running)  // the last install's outcome, once (Android)
export 'client_update_web.dart' if (dart.library.io) 'client_update_io.dart';
