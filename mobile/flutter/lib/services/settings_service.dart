import 'dart:convert';
import 'package:shared_preferences/shared_preferences.dart';
import '../models/app_settings.dart';

class SettingsService {
  static const String _keySettings = 'adesh_app_settings';

  Future<AppSettings> loadSettings() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final str = prefs.getString(_keySettings);
      if (str != null) {
        final Map<String, dynamic> jsonMap = json.decode(str) as Map<String, dynamic>;
        return AppSettings.fromJson(jsonMap);
      }
    } catch (_) {}
    return AppSettings();
  }

  Future<void> saveSettings(AppSettings settings) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final str = json.encode(settings.toJson());
      await prefs.setString(_keySettings, str);
    } catch (_) {}
  }
}
