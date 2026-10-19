import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'hubs/hub_client.dart';
import 'hubs/hub_profile.dart';
import 'screens/hubs_screen.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final prefs = await SharedPreferences.getInstance();
  runApp(ProtectorateApp(store: HubStore(prefs)));
}

class ProtectorateApp extends StatelessWidget {
  const ProtectorateApp({super.key, required this.store, this.clientFactory});

  final HubStore store;
  final HubClient Function(HubProfile)? clientFactory;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Protectorate',
      theme: ThemeData(
        colorSchemeSeed: Colors.indigo,
        brightness: Brightness.light,
        useMaterial3: true,
      ),
      darkTheme: ThemeData(
        colorSchemeSeed: Colors.indigo,
        brightness: Brightness.dark,
        useMaterial3: true,
      ),
      home: HubsScreen(
        store: store,
        clientFactory: clientFactory ?? HubClient.new,
      ),
    );
  }
}
