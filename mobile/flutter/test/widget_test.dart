import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:adesh_mobile/bridge/adesh_bridge_types.dart';
import 'package:adesh_mobile/syntax/adesh_code_controller.dart';
import 'package:adesh_mobile/syntax/adesh_syntax.dart';
import 'package:flutter/material.dart';

import 'package:adesh_mobile/screens/home_screen.dart';
import 'package:adesh_mobile/theme/app_theme.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('AdeshLang HomeScreen launches successfully', (WidgetTester tester) async {
    SharedPreferences.setMockInitialValues({});
    await tester.pumpWidget(
      MaterialApp(
        theme: AppTheme.darkTheme,
        home: const HomeScreen(),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 500));

    expect(find.text('Adesh Editor'), findsAtLeastNWidgets(1));
    expect(find.text('Recent'), findsOneWidget);
    expect(find.text('My Files'), findsOneWidget);
  });

  test('AdeshSyntax highlighting tokens', () {
    const code = 'fn main() { let x = 42; print("Hello"); }';
    final span = AdeshSyntax.highlight(code, const TextStyle());
    expect(span.children, isNotEmpty);
  });

  test('AdeshCodeController undo, redo, and auto-close', () {
    final controller = AdeshCodeController(text: 'fn test()');
    controller.handleAutoClose('(');
    expect(controller.text, contains(')'));

    controller.insertSnippet(' {}');
    expect(controller.text, contains('{}'));

    expect(controller.canUndo, isTrue);
    controller.undo();
    expect(controller.canRedo, isTrue);
    controller.redo();
  });

  test('ExecutionResult and Diagnostic serialization', () {
    final diagJson = {
      'severity': 'error',
      'message': 'Type mismatch',
      'line': 4,
      'column': 9,
      'length': 1,
      'code': 'TypeError',
    };
    final diag = Diagnostic.fromJson(diagJson);
    expect(diag.severity, DiagnosticSeverity.error);
    expect(diag.message, 'Type mismatch');
    expect(diag.line, 4);
    expect(diag.column, 9);

    final resJson = {
      'status': 'completed',
      'stdout': 'Hello, World!\n',
      'stderr': '',
      'execution_time_ms': 1.25,
      'diagnostics': [diagJson],
      'backend_used': 'Interpreter',
    };
    final res = ExecutionResult.fromJson(resJson);
    expect(res.status, ExecutionStatus.completed);
    expect(res.stdout, 'Hello, World!\n');
    expect(res.backendUsed, 'Interpreter');
  });
}
