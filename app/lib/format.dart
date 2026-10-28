import 'gen/protectorate/v1/sleeve.pb.dart';

String formatBytes(num bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
  var v = bytes.toDouble();
  var i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  final digits = i == 0 || v >= 100 ? 0 : (v >= 10 ? 1 : 2);
  return '${v.toStringAsFixed(digits)} ${units[i]}';
}

String formatAge(Duration d) {
  if (d.isNegative || d.inSeconds < 1) return 'now';
  if (d.inSeconds < 90) return '${d.inSeconds}s ago';
  if (d.inMinutes < 90) return '${d.inMinutes}m ago';
  if (d.inHours < 48) return '${d.inHours}h ago';
  return '${d.inDays}d ago';
}

String formatUptime(int seconds) {
  final d = Duration(seconds: seconds);
  if (d.inDays > 0) return '${d.inDays}d ${d.inHours % 24}h';
  if (d.inHours > 0) return '${d.inHours}h ${d.inMinutes % 60}m';
  return '${d.inMinutes}m';
}

String harnessLabel(Harness h) =>
    h.name.replaceFirst('HARNESS_', '').toLowerCase();
