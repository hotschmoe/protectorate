import 'package:connectrpc/connect.dart';
import 'package:connectrpc/web.dart' as impl;

HttpClient createPlatformHttpClient() => impl.createHttpClient();
