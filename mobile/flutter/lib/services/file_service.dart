import 'dart:convert';
import 'dart:io';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import '../models/editor_file.dart';

class FileService {
  static const String _keyRecentFiles = 'adesh_recent_files';

  Future<Directory> get _documentsDir async {
    return await getApplicationDocumentsDirectory();
  }

  Future<EditorFile> createNewFile({String name = 'untitled.adesh', String content = ''}) async {
    final dir = await _documentsDir;
    String safeName = name.endsWith('.adesh') ? name : '$name.adesh';
    final file = File('${dir.path}/$safeName');
    await file.writeAsString(content);

    final editorFile = EditorFile(
      name: safeName,
      path: file.path,
      content: content,
      isModified: false,
    );

    await addToRecent(editorFile);
    return editorFile;
  }

  Future<void> saveFile(EditorFile editorFile) async {
    if (editorFile.path.isNotEmpty) {
      final file = File(editorFile.path);
      await file.writeAsString(editorFile.content);
      editorFile.isModified = false;
      editorFile.lastModified = DateTime.now();
      await addToRecent(editorFile);
    }
  }

  Future<void> deleteFile(String path) async {
    final file = File(path);
    if (await file.exists()) {
      await file.delete();
    }
    await removeFromRecent(path);
  }

  Future<List<EditorFile>> getRecentFiles() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final list = prefs.getStringList(_keyRecentFiles) ?? [];
      final List<EditorFile> result = [];

      for (final str in list) {
        try {
          final jsonMap = json.decode(str) as Map<String, dynamic>;
          final ef = EditorFile.fromJson(jsonMap);
          final file = File(ef.path);
          if (await file.exists()) {
            ef.content = await file.readAsString();
            result.add(ef);
          }
        } catch (_) {}
      }
      return result;
    } catch (_) {
      return [];
    }
  }

  Future<void> addToRecent(EditorFile editorFile) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final list = prefs.getStringList(_keyRecentFiles) ?? [];

      final List<Map<String, dynamic>> items = [];
      for (final str in list) {
        try {
          final map = json.decode(str) as Map<String, dynamic>;
          if (map['path'] != editorFile.path) {
            items.add(map);
          }
        } catch (_) {}
      }

      items.insert(0, editorFile.toJson());
      final updatedList = items.take(15).map((e) => json.encode(e)).toList();
      await prefs.setStringList(_keyRecentFiles, updatedList);
    } catch (_) {}
  }

  Future<void> removeFromRecent(String path) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final list = prefs.getStringList(_keyRecentFiles) ?? [];
      final List<String> updatedList = [];

      for (final str in list) {
        try {
          final map = json.decode(str) as Map<String, dynamic>;
          if (map['path'] != path) {
            updatedList.add(str);
          }
        } catch (_) {}
      }
      await prefs.setStringList(_keyRecentFiles, updatedList);
    } catch (_) {}
  }

  /// Returns all .adesh files saved in the app's documents directory.
  Future<List<EditorFile>> getAllFiles() async {
    try {
      final dir = await _documentsDir;
      final files = dir.listSync().whereType<File>().where((f) => f.path.endsWith('.adesh')).toList();
      files.sort((a, b) => b.statSync().modified.compareTo(a.statSync().modified));
      return files.map((f) {
        final name = f.path.split(Platform.pathSeparator).last;
        return EditorFile(name: name, path: f.path, content: '');
      }).toList();
    } catch (_) {
      return [];
    }
  }

  /// Reads and returns the content of a file at [path].
  Future<String> getFileContent(String path) async {
    try {
      return await File(path).readAsString();
    } catch (_) {
      return '';
    }
  }
}
