import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'package:ffi/ffi.dart';

import 'adesh_bridge_types.dart';

typedef NativeCreateSession = Pointer<Void> Function();
typedef DartCreateSession = Pointer<Void> Function();

typedef NativeDestroySession = Void Function(Pointer<Void> session);
typedef DartDestroySession = void Function(Pointer<Void> session);

typedef NativeRun = Pointer<Utf8> Function(
    Pointer<Void> session, Pointer<Utf8> code, Pointer<Utf8> filename);
typedef DartRun = Pointer<Utf8> Function(
    Pointer<Void> session, Pointer<Utf8> code, Pointer<Utf8> filename);

typedef NativeCheck = Pointer<Utf8> Function(
    Pointer<Void> session, Pointer<Utf8> code, Pointer<Utf8> filename);
typedef DartCheck = Pointer<Utf8> Function(
    Pointer<Void> session, Pointer<Utf8> code, Pointer<Utf8> filename);

typedef NativeCancel = Int32 Function(Pointer<Void> session);
typedef DartCancel = int Function(Pointer<Void> session);

typedef NativeFormat = Pointer<Utf8> Function(Pointer<Utf8> code);
typedef DartFormat = Pointer<Utf8> Function(Pointer<Utf8> code);

typedef NativeFreeString = Void Function(Pointer<Utf8> ptr);
typedef DartFreeString = void Function(Pointer<Utf8> ptr);

typedef NativeGetVersion = Pointer<Utf8> Function();
typedef DartGetVersion = Pointer<Utf8> Function();

class AdeshNativeBridge {
  static final AdeshNativeBridge _instance = AdeshNativeBridge._internal();
  factory AdeshNativeBridge() => _instance;

  DynamicLibrary? _lib;
  bool _isAvailable = false;

  DartCreateSession? _createSession;
  DartDestroySession? _destroySession;
  DartRun? _run;
  DartCheck? _check;
  DartCancel? _cancel;
  DartFormat? _format;
  DartFreeString? _freeString;
  DartGetVersion? _getVersion;

  AdeshNativeBridge._internal() {
    _initLibrary();
  }

  bool get isAvailable => _isAvailable;

  void _initLibrary() {
    try {
      if (Platform.isAndroid || Platform.isLinux) {
        try {
          _lib = DynamicLibrary.open('libadesh_mobile_bridge.so');
        } catch (_) {
          _lib = DynamicLibrary.process();
        }
      } else if (Platform.isIOS || Platform.isMacOS) {
        _lib = DynamicLibrary.process();
      } else if (Platform.isWindows) {
        try {
          _lib = DynamicLibrary.open('adesh_mobile_bridge.dll');
        } catch (_) {
          _lib = DynamicLibrary.process();
        }
      } else {
        _lib = DynamicLibrary.process();
      }

      if (_lib != null) {
        _createSession = _lib!
            .lookup<NativeFunction<NativeCreateSession>>('adesh_session_create')
            .asFunction();
        _destroySession = _lib!
            .lookup<NativeFunction<NativeDestroySession>>('adesh_session_destroy')
            .asFunction();
        _run = _lib!
            .lookup<NativeFunction<NativeRun>>('adesh_run')
            .asFunction();
        _check = _lib!
            .lookup<NativeFunction<NativeCheck>>('adesh_check')
            .asFunction();
        _cancel = _lib!
            .lookup<NativeFunction<NativeCancel>>('adesh_cancel')
            .asFunction();
        _format = _lib!
            .lookup<NativeFunction<NativeFormat>>('adesh_format')
            .asFunction();
        _freeString = _lib!
            .lookup<NativeFunction<NativeFreeString>>('adesh_free_string')
            .asFunction();
        _getVersion = _lib!
            .lookup<NativeFunction<NativeGetVersion>>('adesh_get_version')
            .asFunction();

        _isAvailable = true;
      }
    } catch (_) {
      _isAvailable = false;
    }
  }

  Pointer<Void>? createSession() {
    if (!_isAvailable || _createSession == null) return null;
    return _createSession!();
  }

  void destroySession(Pointer<Void>? session) {
    if (!_isAvailable || _destroySession == null || session == null) return;
    _destroySession!(session);
  }

  ExecutionResult runCode(Pointer<Void>? session, String code, String filename) {
    if (!_isAvailable || _run == null || _freeString == null) {
      return ExecutionResult.failure(
        'Native AdeshLang bridge library (libadesh_mobile_bridge) is not loaded for this architecture.',
      );
    }

    final codePtr = code.toNativeUtf8();
    final fnPtr = filename.toNativeUtf8();
    Pointer<Utf8>? resultPtr;

    try {
      final sessPtr = session ?? nullptr;
      resultPtr = _run!(sessPtr, codePtr, fnPtr);
      final jsonStr = resultPtr.toDartString();
      final Map<String, dynamic> jsonMap = json.decode(jsonStr) as Map<String, dynamic>;
      return ExecutionResult.fromJson(jsonMap);
    } catch (e) {
      return ExecutionResult.failure('FFI Execution Error: $e');
    } finally {
      calloc.free(codePtr);
      calloc.free(fnPtr);
      if (resultPtr != null && resultPtr != nullptr) {
        _freeString!(resultPtr);
      }
    }
  }

  List<Diagnostic> checkCode(Pointer<Void>? session, String code, String filename) {
    if (!_isAvailable || _check == null || _freeString == null) {
      return [];
    }

    final codePtr = code.toNativeUtf8();
    final fnPtr = filename.toNativeUtf8();
    Pointer<Utf8>? resultPtr;

    try {
      final sessPtr = session ?? nullptr;
      resultPtr = _check!(sessPtr, codePtr, fnPtr);
      final jsonStr = resultPtr.toDartString();
      final List<dynamic> jsonList = json.decode(jsonStr) as List<dynamic>;
      return jsonList
          .map((d) => Diagnostic.fromJson(d as Map<String, dynamic>))
          .toList();
    } catch (e) {
      return [Diagnostic(severity: DiagnosticSeverity.error, message: 'Check Error: $e', line: 1, column: 1)];
    } finally {
      calloc.free(codePtr);
      calloc.free(fnPtr);
      if (resultPtr != null && resultPtr != nullptr) {
        _freeString!(resultPtr);
      }
    }
  }

  void cancelExecution(Pointer<Void>? session) {
    if (!_isAvailable || _cancel == null || session == null) return;
    _cancel!(session);
  }

  String formatCode(String code) {
    if (!_isAvailable || _format == null || _freeString == null) {
      return code;
    }

    final codePtr = code.toNativeUtf8();
    Pointer<Utf8>? resultPtr;

    try {
      resultPtr = _format!(codePtr);
      return resultPtr.toDartString();
    } catch (_) {
      return code;
    } finally {
      calloc.free(codePtr);
      if (resultPtr != null && resultPtr != nullptr) {
        _freeString!(resultPtr);
      }
    }
  }

  String getVersion() {
    if (!_isAvailable || _getVersion == null || _freeString == null) {
      return 'AdeshLang v0.3.0 (Dart Service)';
    }

    Pointer<Utf8>? resultPtr;
    try {
      resultPtr = _getVersion!();
      return resultPtr.toDartString();
    } catch (_) {
      return 'AdeshLang v0.3.0';
    } finally {
      if (resultPtr != null && resultPtr != nullptr) {
        _freeString!(resultPtr);
      }
    }
  }
}
