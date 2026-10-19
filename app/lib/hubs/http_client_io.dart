import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as impl;

HttpClient createPlatformHttpClient() => impl.createHttpClient(io.HttpClient());
