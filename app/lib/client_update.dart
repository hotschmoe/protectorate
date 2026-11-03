// The app updating itself to the hub's release, per platform: Android and Windows download
// their build from the hub and install it; on web the hub serves the app, so an update is a
// reload.
//
//   const bool reloadsFromHub                     // true on web
//   String? clientAsset()                         // this build's name in HubInfo.clients, or null
//   void reloadPage()                             // web only
//   Future<String> downloadClient(Uri url, String? token,
//       void Function(int got, int total) progress)   // the downloaded file's path
//   Future<String> installClient(String path)     // what happens next; Windows exits instead
export 'client_update_web.dart' if (dart.library.io) 'client_update_io.dart';
