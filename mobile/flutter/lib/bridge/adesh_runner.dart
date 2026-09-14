import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';

import 'adesh_bridge.dart';
import 'adesh_bridge_types.dart';

class AdeshRunner {
  Future<List<Diagnostic>> checkCode(String code, {String filename = 'main.adesh'}) async {
    if (code.trim().isEmpty) return [];

    return await compute(_checkCodeTask, {
      'code': code,
      'filename': filename,
    });
  }

  Future<ExecutionResult> runCode(
    String code, {
    String filename = 'main.adesh',
    String stdin = '',
  }) async {
    if (code.trim().isEmpty) {
      return ExecutionResult.failure('Cannot execute empty file');
    }

    return await compute(_runCodeTask, {
      'code': code,
      'filename': filename,
      'stdin': stdin,
    });
  }

  Future<String> formatCode(String code) async {
    if (code.trim().isEmpty) return code;

    return await compute(_formatCodeTask, code);
  }

  static List<Diagnostic> _checkCodeTask(Map<String, dynamic> args) {
    final code = args['code'] as String;
    final filename = args['filename'] as String;
    final bridge = AdeshNativeBridge();
    final session = bridge.createSession();
    try {
      return bridge.checkCode(session, code, filename);
    } finally {
      bridge.destroySession(session);
    }
  }

  static ExecutionResult _runCodeTask(Map<String, dynamic> args) {
    String code = args['code'] as String;
    final filename = args['filename'] as String;
    final stdin = (args['stdin'] as String?)?.trim() ?? '';

    // If stdin inputs are provided, inject mock input setup at the top of script
    if (stdin.isNotEmpty) {
      final lines = stdin.split('\n');
      final jsonLines = json.encode(lines);
      code = 'input.mock($jsonLines);\n$code';
    }

    final bridge = AdeshNativeBridge();
    final session = bridge.createSession();
    try {
      return bridge.runCode(session, code, filename);
    } finally {
      bridge.destroySession(session);
    }
  }

  static String _formatCodeTask(String code) {
    final bridge = AdeshNativeBridge();
    return bridge.formatCode(code);
  }
}
