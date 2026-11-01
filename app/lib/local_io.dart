final Map<String, String> _settings = {};

String? loadSetting(String key) => _settings[key];

void saveSetting(String key, String? value) {
  if (value == null || value.isEmpty) {
    _settings.remove(key);
  } else {
    _settings[key] = value;
  }
}

String? pageOrigin() => null;

void dropQueryParam(String key) {}
