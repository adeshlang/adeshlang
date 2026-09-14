import 'dart:async';
import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:share_plus/share_plus.dart';

import '../bridge/adesh_bridge_types.dart';
import '../bridge/adesh_runner.dart';
import '../models/app_settings.dart';
import '../models/editor_file.dart';
import '../services/file_service.dart';
import '../syntax/adesh_code_controller.dart';
import '../theme/app_theme.dart';
import '../widgets/advanced_viewer_dialog.dart';
import '../widgets/autocomplete_popup.dart';
import '../widgets/console_panel.dart';
import '../widgets/diagnostic_bar.dart';
import '../widgets/keyboard_accessory_bar.dart';
import '../widgets/line_numbers.dart';
import 'settings_screen.dart';

class EditorScreen extends StatefulWidget {
  final EditorFile file;
  final AppSettings settings;

  const EditorScreen({
    super.key,
    required this.file,
    required this.settings,
  });

  @override
  State<EditorScreen> createState() => _EditorScreenState();
}

class _EditorScreenState extends State<EditorScreen> {
  late EditorFile _file;
  late AppSettings _settings;
  late AdeshCodeController _codeController;
  final AdeshRunner _runner = AdeshRunner();
  final FileService _fileService = FileService();

  final ScrollController _codeScrollController = ScrollController();
  final ScrollController _lineNumScrollController = ScrollController();

  ExecutionResult? _lastResult;
  List<Diagnostic> _diagnostics = [];
  bool _isRunning = false;
  bool _showConsole = false;
  double _consoleHeight = 260.0;
  String _programStdin = '';

  List<AutocompleteSuggestion> _suggestions = [];
  String _currentPrefix = '';
  Timer? _autosaveTimer;

  final List<AutocompleteSuggestion> _allSuggestions = [
    AutocompleteSuggestion(label: 'fn', insertText: 'fn ', detail: 'Function declaration', type: 'keyword'),
    AutocompleteSuggestion(label: 'let', insertText: 'let ', detail: 'Immutable variable binding', type: 'keyword'),
    AutocompleteSuggestion(label: 'let mut', insertText: 'let mut ', detail: 'Mutable variable binding', type: 'keyword'),
    AutocompleteSuggestion(label: 'if', insertText: 'if ', detail: 'Conditional if', type: 'keyword'),
    AutocompleteSuggestion(label: 'else', insertText: 'else ', detail: 'Conditional else', type: 'keyword'),
    AutocompleteSuggestion(label: 'while', insertText: 'while ', detail: 'While loop', type: 'keyword'),
    AutocompleteSuggestion(label: 'for', insertText: 'for ', detail: 'For loop', type: 'keyword'),
    AutocompleteSuggestion(label: 'print', insertText: 'print(', detail: 'Print to stdout', type: 'function'),
    AutocompleteSuggestion(label: 'println', insertText: 'println(', detail: 'Print with newline to stdout', type: 'function'),
    AutocompleteSuggestion(label: 'print("{}")', insertText: 'print("{}", ', detail: 'Formatted print with placeholder', type: 'function'),
    AutocompleteSuggestion(label: 'println("{}")', insertText: 'println("{}", ', detail: 'Formatted println with placeholder', type: 'function'),
    AutocompleteSuggestion(label: 'print("", {})', insertText: 'print("", {});', detail: 'Print format string with map / block args', type: 'function'),
    AutocompleteSuggestion(label: 'eprint', insertText: 'eprint(', detail: 'Print to stderr', type: 'function'),
    AutocompleteSuggestion(label: 'eprintln', insertText: 'eprintln(', detail: 'Print with newline to stderr', type: 'function'),
    AutocompleteSuggestion(label: 'printf', insertText: 'printf(', detail: 'Formatted print to stdout', type: 'function'),
    AutocompleteSuggestion(label: 'input', insertText: 'input(', detail: 'Read user input from stdin', type: 'function'),
    AutocompleteSuggestion(label: 'input("prompt")', insertText: 'input("', detail: 'Prompt and read user input', type: 'function'),
    AutocompleteSuggestion(label: 'struct', insertText: 'struct ', detail: 'Struct declaration', type: 'keyword'),
    AutocompleteSuggestion(label: 'import', insertText: 'import ', detail: 'Module import statement', type: 'keyword'),
    AutocompleteSuggestion(label: 'return', insertText: 'return ', detail: 'Return statement', type: 'keyword'),
    AutocompleteSuggestion(label: 'match', insertText: 'match ', detail: 'Pattern matching', type: 'keyword'),
    AutocompleteSuggestion(label: 'async', insertText: 'async ', detail: 'Async function', type: 'keyword'),
    AutocompleteSuggestion(label: 'await', insertText: 'await ', detail: 'Await expression', type: 'keyword'),
    AutocompleteSuggestion(label: 'len', insertText: 'len(', detail: 'Get length of collection', type: 'function'),
    AutocompleteSuggestion(label: 'type', insertText: 'type(', detail: 'Get runtime type name', type: 'function'),
    AutocompleteSuggestion(label: 'assert', insertText: 'assert(', detail: 'Assert condition is true', type: 'function'),
    AutocompleteSuggestion(label: 'assert_eq', insertText: 'assert_eq(', detail: 'Assert two values are equal', type: 'function'),
    AutocompleteSuggestion(label: 'true', insertText: 'true', detail: 'Boolean true', type: 'keyword'),
    AutocompleteSuggestion(label: 'false', insertText: 'false', detail: 'Boolean false', type: 'keyword'),
    AutocompleteSuggestion(label: 'nil', insertText: 'nil', detail: 'Null / nil value', type: 'keyword'),
  ];

  @override
  void initState() {
    super.initState();
    _file = widget.file;
    _settings = widget.settings;
    _codeController = AdeshCodeController(
      text: _file.content,
      enableSyntaxHighlighting: _settings.enableSyntaxHighlighting,
      autoCloseBrackets: _settings.autoCloseBrackets,
    );

    _codeController.addListener(() {
      if (_codeController.text != _file.content) {
        setState(() {
          _file.content = _codeController.text;
          _file.isModified = true;
          if (_showConsole) {
            _showConsole = false;
          }
        });

        if (_settings.enableAutosave) {
          _autosaveTimer?.cancel();
          _autosaveTimer = Timer(const Duration(milliseconds: 1500), () {
            if (mounted && _file.isModified) {
              _fileService.saveFile(_file);
              setState(() {
                _file.isModified = false;
              });
            }
          });
        }
      }
      _checkAutocomplete();
    });

    _codeScrollController.addListener(() {
      if (_lineNumScrollController.hasClients) {
        _lineNumScrollController.jumpTo(_codeScrollController.offset);
      }
    });
  }

  void _checkAutocomplete() {
    if (!_settings.enableAutocomplete) {
      if (_suggestions.isNotEmpty) setState(() => _suggestions = []);
      return;
    }

    final text = _codeController.text;
    final sel = _codeController.selection;
    if (!sel.isValid || !sel.isCollapsed) {
      if (_suggestions.isNotEmpty) setState(() => _suggestions = []);
      return;
    }

    final offset = sel.baseOffset;
    if (offset == 0) {
      if (_suggestions.isNotEmpty) setState(() => _suggestions = []);
      return;
    }

    final wordMatch = RegExp(r'[a-zA-Z_]\w*$').firstMatch(text.substring(0, offset));
    if (wordMatch != null) {
      final word = wordMatch.group(0)!;
      if (word.length >= 2) {
        final matches = _allSuggestions
            .where((s) => s.label.toLowerCase().startsWith(word.toLowerCase()) && s.label.toLowerCase() != word.toLowerCase())
            .toList();
        if (matches.isNotEmpty) {
          _currentPrefix = word;
          setState(() => _suggestions = matches);
          return;
        }
      }
    }
    if (_suggestions.isNotEmpty) setState(() => _suggestions = []);
  }

  void _applySuggestion(AutocompleteSuggestion item) {
    if (_currentPrefix.isNotEmpty) {
      _codeController.applyCompletion(_currentPrefix, item.insertText);
    } else {
      _codeController.insertSnippet(item.insertText);
    }
    setState(() {
      _suggestions = [];
      _currentPrefix = '';
    });
  }

  final FocusNode _focusNode = FocusNode();

  @override
  void dispose() {
    _autosaveTimer?.cancel();
    _focusNode.dispose();
    _codeController.dispose();
    _codeScrollController.dispose();
    _lineNumScrollController.dispose();
    super.dispose();
  }

  Future<void> _saveFile() async {
    _autosaveTimer?.cancel();
    if (_settings.enableAutoFormat) {
      final formatted = await _runner.formatCode(_codeController.text);
      if (formatted != _codeController.text) {
        _codeController.text = formatted;
        _file.content = formatted;
      }
    }
    await _fileService.saveFile(_file);
    setState(() {
      _file.isModified = false;
    });
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text('Saved ${_file.name}')),
    );
  }

  Future<void> _runCode() async {
    _autosaveTimer?.cancel();
    if (_settings.enableAutoFormat) {
      final formatted = await _runner.formatCode(_codeController.text);
      if (formatted != _codeController.text) {
        _codeController.text = formatted;
        _file.content = formatted;
        await _fileService.saveFile(_file);
      }
    }

    setState(() {
      _isRunning = true;
      if (_settings.autoOpenConsole) {
        _showConsole = true;
      }
      _suggestions = [];
    });

    final result = await _runner.runCode(
      _codeController.text,
      filename: _file.name,
      stdin: _programStdin,
    );

    if (!mounted) return;
    setState(() {
      _lastResult = result;
      _diagnostics = result.diagnostics;
      _isRunning = false;
      // Always open console so the user sees the result
      _showConsole = true;
    });
  }

  void _stopExecution() {
    setState(() {
      _isRunning = false;
      _lastResult = ExecutionResult.cancelled();
    });
  }

  Future<void> _checkCode() async {
    final diags = await _runner.checkCode(_codeController.text, filename: _file.name);
    if (!mounted) return;
    setState(() {
      _diagnostics = diags;
    });
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          diags.isEmpty ? '✓ Check passed! No syntax or type errors.' : 'Found ${diags.length} diagnostic issue(s)',
        ),
        backgroundColor: diags.isEmpty ? AppTheme.secondary : AppTheme.error,
      ),
    );
  }

  Future<void> _formatCode() async {
    final formatted = await _runner.formatCode(_codeController.text);
    if (formatted != _codeController.text) {
      _codeController.text = formatted;
      _saveFile();
    }
  }

  void _moveCursorLeft() {
    final sel = _codeController.selection;
    if (!sel.isValid) return;
    final newOffset = (sel.baseOffset - 1).clamp(0, _codeController.text.length);
    _codeController.value = _codeController.value.copyWith(
      selection: TextSelection.collapsed(offset: newOffset),
      composing: TextRange.empty,
    );
    _focusNode.requestFocus();
  }

  void _moveCursorRight() {
    final sel = _codeController.selection;
    if (!sel.isValid) return;
    final newOffset = (sel.baseOffset + 1).clamp(0, _codeController.text.length);
    _codeController.value = _codeController.value.copyWith(
      selection: TextSelection.collapsed(offset: newOffset),
      composing: TextRange.empty,
    );
    _focusNode.requestFocus();
  }

  void _moveCursorHome() {
    _codeController.moveToLineStart();
    _focusNode.requestFocus();
  }

  void _moveCursorEnd() {
    _codeController.moveToLineEnd();
    _focusNode.requestFocus();
  }

  void _moveCursorUp() {
    final text = _codeController.text;
    final sel = _codeController.selection;
    final offset = sel.isValid ? sel.baseOffset.clamp(0, text.length) : 0;

    // Find start of current line
    final currentLineStart = offset == 0 ? 0 : text.lastIndexOf('\n', offset - 1) + 1;
    if (currentLineStart == 0) {
      // Already on first line — move to start of text
      _codeController.value = _codeController.value.copyWith(
        selection: const TextSelection.collapsed(offset: 0),
        composing: TextRange.empty,
      );
      _focusNode.requestFocus();
      return;
    }

    final col = offset - currentLineStart;
    final prevLineEnd = currentLineStart - 1; // index of the newline before current line
    final prevLineStart = prevLineEnd == 0 ? 0 : text.lastIndexOf('\n', prevLineEnd - 1) + 1;
    final prevLineLen = prevLineEnd - prevLineStart;

    final targetCol = col.clamp(0, prevLineLen);
    final newOffset = prevLineStart + targetCol;

    _codeController.value = _codeController.value.copyWith(
      selection: TextSelection.collapsed(offset: newOffset),
      composing: TextRange.empty,
    );
    _focusNode.requestFocus();
  }

  void _moveCursorDown() {
    final text = _codeController.text;
    final sel = _codeController.selection;
    final offset = sel.isValid ? sel.baseOffset.clamp(0, text.length) : 0;

    final currentLineStart = offset == 0 ? 0 : text.lastIndexOf('\n', offset - 1) + 1;
    final col = offset - currentLineStart;

    final nextNl = text.indexOf('\n', offset);
    if (nextNl == -1) {
      // Already on last line — move to end of text
      _codeController.value = _codeController.value.copyWith(
        selection: TextSelection.collapsed(offset: text.length),
        composing: TextRange.empty,
      );
      _focusNode.requestFocus();
      return;
    }

    final nextLineStart = nextNl + 1;
    final nextLineEnd = text.indexOf('\n', nextLineStart);
    final nextLineLen = (nextLineEnd == -1 ? text.length : nextLineEnd) - nextLineStart;

    final targetCol = col.clamp(0, nextLineLen);
    final newOffset = nextLineStart + targetCol;

    _codeController.value = _codeController.value.copyWith(
      selection: TextSelection.collapsed(offset: newOffset),
      composing: TextRange.empty,
    );
    _focusNode.requestFocus();
  }

  void _showAdvancedOptions() {
    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (ctx) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 36,
                height: 4,
                margin: const EdgeInsets.only(bottom: 16),
                decoration: BoxDecoration(
                  color: AppTheme.border,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),
              const Padding(
                padding: EdgeInsets.symmetric(horizontal: 16),
                child: Row(
                  children: [
                    Icon(Icons.account_tree_rounded, color: AppTheme.primary, size: 20),
                    SizedBox(width: 8),
                    Text('Advanced Compiler Representations', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
                  ],
                ),
              ),
              const SizedBox(height: 8),
              ListTile(
                leading: const Icon(Icons.code_rounded, color: AppTheme.secondary),
                title: const Text('AST (Abstract Syntax Tree)'),
                subtitle: const Text('Parsed hierarchical syntax nodes and declarations'),
                trailing: const Icon(Icons.chevron_right_rounded),
                onTap: () {
                  Navigator.pop(ctx);
                  AdvancedViewerDialog.show(context, sourceCode: _codeController.text, filename: _file.name, initialMode: IrViewMode.ast);
                },
              ),
              ListTile(
                leading: const Icon(Icons.transform_rounded, color: AppTheme.primary),
                title: const Text('HIR (High-Level IR)'),
                subtitle: const Text('Typed, desugared representation with ownership checking'),
                trailing: const Icon(Icons.chevron_right_rounded),
                onTap: () {
                  Navigator.pop(ctx);
                  AdvancedViewerDialog.show(context, sourceCode: _codeController.text, filename: _file.name, initialMode: IrViewMode.hir);
                },
              ),
              ListTile(
                leading: const Icon(Icons.hub_rounded, color: AppTheme.accent),
                title: const Text('IR (Virtual SSA IR)'),
                subtitle: const Text('Basic blocks, control flow graph, SSA registers'),
                trailing: const Icon(Icons.chevron_right_rounded),
                onTap: () {
                  Navigator.pop(ctx);
                  AdvancedViewerDialog.show(context, sourceCode: _codeController.text, filename: _file.name, initialMode: IrViewMode.ir);
                },
              ),
              ListTile(
                leading: const Icon(Icons.memory_rounded, color: AppTheme.warning),
                title: const Text('LIR (Low-Level IR)'),
                subtitle: const Text('Register allocations & machine calling conventions'),
                trailing: const Icon(Icons.chevron_right_rounded),
                onTap: () {
                  Navigator.pop(ctx);
                  AdvancedViewerDialog.show(context, sourceCode: _codeController.text, filename: _file.name, initialMode: IrViewMode.lir);
                },
              ),
              ListTile(
                leading: const Icon(Icons.layers_rounded, color: Colors.purpleAccent),
                title: const Text('MLIR (Multi-Level IR)'),
                subtitle: const Text('GPU vectorization & parallel execution dialects'),
                trailing: const Icon(Icons.chevron_right_rounded),
                onTap: () {
                  Navigator.pop(ctx);
                  AdvancedViewerDialog.show(context, sourceCode: _codeController.text, filename: _file.name, initialMode: IrViewMode.mlir);
                },
              ),
            ],
          ),
        ),
      ),
    );
  }

  Future<void> _openSettings() async {
    await Navigator.push(
      context,
      MaterialPageRoute(
        builder: (ctx) => SettingsScreen(
          settings: _settings,
          onSave: (newSettings) {
            setState(() {
              _settings = newSettings;
              _codeController.enableSyntaxHighlighting = newSettings.enableSyntaxHighlighting;
              _codeController.autoCloseBrackets = newSettings.autoCloseBrackets;
              _codeController.tabSize = newSettings.tabSize;
            });
          },
        ),
      ),
    );
    setState(() {
      _codeController.enableSyntaxHighlighting = _settings.enableSyntaxHighlighting;
      _codeController.autoCloseBrackets = _settings.autoCloseBrackets;
      _codeController.tabSize = _settings.tabSize;
    });
  }

  void _showFindReplaceDialog() {
    final findController = TextEditingController();
    final replaceController = TextEditingController();

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Find & Replace'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: findController,
              decoration: const InputDecoration(labelText: 'Find', border: OutlineInputBorder()),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: replaceController,
              decoration: const InputDecoration(labelText: 'Replace with', border: OutlineInputBorder()),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () {
              final find = findController.text;
              final replace = replaceController.text;
              if (find.isNotEmpty) {
                final newText = _codeController.text.replaceAll(find, replace);
                _codeController.text = newText;
              }
              Navigator.pop(context);
            },
            child: const Text('Replace All'),
          ),
        ],
      ),
    );
  }

  void _showGoToLineDialog() {
    final lineController = TextEditingController();
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Go to Line'),
        content: TextField(
          controller: lineController,
          keyboardType: TextInputType.number,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Line Number', border: OutlineInputBorder()),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
          ElevatedButton(
            onPressed: () {
              final l = int.tryParse(lineController.text);
              if (l != null && l > 0) {
                _jumpToLine(l);
              }
              Navigator.pop(context);
            },
            child: const Text('Go'),
          ),
        ],
      ),
    );
  }

  void _confirmClearCode() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clear Code'),
        content: const Text('Are you sure you want to clear all code in this file? You can still use Undo if needed.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            style: ElevatedButton.styleFrom(backgroundColor: AppTheme.error),
            onPressed: () {
              Navigator.pop(context);
              _codeController.pushUndoState();
              _codeController.text = '';
              setState(() {
                _file.content = '';
                _file.isModified = true;
              });
            },
            child: const Text('Clear All'),
          ),
        ],
      ),
    );
  }

  void _clearConsole() {
    setState(() {
      _lastResult = null;
      _diagnostics = [];
    });
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('Console and diagnostics cleared')),
    );
  }

  void _shareCode() {
    Share.share(
      _codeController.text,
      subject: _file.name,
    );
  }

  void _showFileStatistics() {
    final text = _codeController.text;
    final lines = '\n'.allMatches(text).length + (text.isEmpty ? 0 : 1);
    final chars = text.length;
    final words = RegExp(r'\S+').allMatches(text).length;
    final fns = RegExp(r'\bfn\s+\w+').allMatches(text).length;
    final bytes = text.codeUnits.length;

    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: AppTheme.surface,
        title: Row(
          children: [
            const Icon(Icons.analytics_outlined, color: AppTheme.secondary),
            const SizedBox(width: 8),
            Text('${_file.name} Stats'),
          ],
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _buildStatRow('Lines of Code', '$lines'),
            _buildStatRow('Words', '$words'),
            _buildStatRow('Characters', '$chars'),
            _buildStatRow('Size', '$bytes bytes'),
            _buildStatRow('Functions', '$fns declared'),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  Widget _buildStatRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(label, style: const TextStyle(color: AppTheme.textSecondary, fontSize: 13)),
          Text(value, style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 13, color: AppTheme.textPrimary)),
        ],
      ),
    );
  }

  void _showInsertTemplateDialog() {
    final templates = [
      {
        'title': 'Hello World Function',
        'desc': 'Standard entrypoint function',
        'code': 'fn main() {\n    print("Hello from AdeshLang!");\n}\n',
      },
      {
        'title': 'Fibonacci Algorithm',
        'desc': 'Recursive computation',
        'code': 'fn fib(n) {\n    if n <= 1 {\n        return n;\n    }\n    return fib(n - 1) + fib(n - 2);\n}\n\nprint(fib(10));\n',
      },
      {
        'title': 'Struct & Constructor',
        'desc': 'Custom data structure',
        'code': 'struct Point {\n    x: i32,\n    y: i32,\n}\n\nlet p = Point { x: 10, y: 20 };\nprint(p.x);\n',
      },
      {
        'title': 'While Loop Counter',
        'desc': 'Iterative loop with accumulator',
        'code': 'let mut count = 0;\nwhile count < 5 {\n    print(count);\n    count = count + 1;\n}\n',
      },
      {
        'title': 'Pattern Match Expression',
        'desc': 'Match statement branches',
        'code': 'let value = 2;\nmatch value {\n    1 => print("One"),\n    2 => print("Two"),\n    _ => print("Other"),\n}\n',
      },
    ];

    showModalBottomSheet(
      context: context,
      backgroundColor: AppTheme.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (context) => ListView(
        padding: const EdgeInsets.all(16),
        shrinkWrap: true,
        children: [
          const Text('Insert Code Template', style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
          const SizedBox(height: 12),
          ...templates.map((tpl) => Card(
            color: AppTheme.editorBackground,
            margin: const EdgeInsets.only(bottom: 8),
            child: ListTile(
              title: Text(tpl['title']!, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13)),
              subtitle: Text(tpl['desc']!, style: const TextStyle(fontSize: 11, color: AppTheme.textSecondary)),
              trailing: const Icon(Icons.add_circle_outline_rounded, color: AppTheme.secondary),
              onTap: () {
                Navigator.pop(context);
                _codeController.insertSnippet(tpl['code']!);
              },
            ),
          )),
        ],
      ),
    );
  }

  void _jumpToLine(int line) {
    final lineOffset = (line - 1) * (_settings.fontSize * 1.4);
    _codeScrollController.animateTo(
      lineOffset.clamp(0.0, _codeScrollController.position.maxScrollExtent),
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
    );
  }

  TextStyle _getEditorTextStyle() {
    switch (_settings.fontFamily) {
      case 'Fira Code':
        return GoogleFonts.firaCode(
          fontSize: _settings.fontSize,
          height: 1.4,
          color: AppTheme.textPrimary,
        );
      case 'Roboto Mono':
        return GoogleFonts.robotoMono(
          fontSize: _settings.fontSize,
          height: 1.4,
          color: AppTheme.textPrimary,
        );
      case 'Courier Prime':
        return GoogleFonts.courierPrime(
          fontSize: _settings.fontSize,
          height: 1.4,
          color: AppTheme.textPrimary,
        );
      case 'JetBrains Mono':
      default:
        return GoogleFonts.jetBrainsMono(
          fontSize: _settings.fontSize,
          height: 1.4,
          color: AppTheme.textPrimary,
        );
    }
  }

  @override
  Widget build(BuildContext context) {
    final lineCount = '\n'.allMatches(_codeController.text).length + 1;

    return Scaffold(
      appBar: AppBar(
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(_file.name, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
                if (_file.isModified) ...[
                  const SizedBox(width: 6),
                  Container(
                    width: 8,
                    height: 8,
                    decoration: const BoxDecoration(color: AppTheme.warning, shape: BoxShape.circle),
                  ),
                ],
              ],
            ),
            const Text('AdeshLang Interpreter', style: TextStyle(fontSize: 11, color: AppTheme.textSecondary)),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.undo_rounded),
            onPressed: _codeController.canUndo ? () => _codeController.undo() : null,
            tooltip: 'Undo',
          ),
          IconButton(
            icon: const Icon(Icons.redo_rounded),
            onPressed: _codeController.canRedo ? () => _codeController.redo() : null,
            tooltip: 'Redo',
          ),
          IconButton(
            icon: const Icon(Icons.save_rounded, color: AppTheme.secondary),
            onPressed: _saveFile,
            tooltip: 'Save',
          ),
          PopupMenuButton<String>(
            onSelected: (value) {
              if (value == 'format') _formatCode();
              if (value == 'check') _checkCode();
              if (value == 'advanced') _showAdvancedOptions();
              if (value == 'template') _showInsertTemplateDialog();
              if (value == 'stats') _showFileStatistics();
              if (value == 'share') _shareCode();
              if (value == 'find') _showFindReplaceDialog();
              if (value == 'goto') _showGoToLineDialog();
              if (value == 'settings') _openSettings();
              if (value == 'clear_code') _confirmClearCode();
              if (value == 'clear_console') _clearConsole();
            },
            itemBuilder: (context) => [
              const PopupMenuItem(
                value: 'format',
                child: Row(
                  children: [
                    Icon(Icons.auto_fix_high_rounded, size: 18, color: AppTheme.secondary),
                    SizedBox(width: 10),
                    Text('Format Code'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'check',
                child: Row(
                  children: [
                    Icon(Icons.spellcheck_rounded, size: 18, color: AppTheme.primary),
                    SizedBox(width: 10),
                    Text('Check Syntax & Types'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'advanced',
                child: Row(
                  children: [
                    Icon(Icons.account_tree_rounded, size: 18, color: Colors.purpleAccent),
                    SizedBox(width: 10),
                    Text('Advanced Options (AST/IR)'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'template',
                child: Row(
                  children: [
                    Icon(Icons.data_object_rounded, size: 18, color: AppTheme.accent),
                    SizedBox(width: 10),
                    Text('Insert Template...'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'stats',
                child: Row(
                  children: [
                    Icon(Icons.analytics_outlined, size: 18, color: AppTheme.textSecondary),
                    SizedBox(width: 10),
                    Text('File Statistics'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'share',
                child: Row(
                  children: [
                    Icon(Icons.share_rounded, size: 18, color: AppTheme.textSecondary),
                    SizedBox(width: 10),
                    Text('Share Code'),
                  ],
                ),
              ),
              const PopupMenuDivider(),
              const PopupMenuItem(
                value: 'find',
                child: Row(
                  children: [
                    Icon(Icons.search_rounded, size: 18, color: AppTheme.textSecondary),
                    SizedBox(width: 10),
                    Text('Find & Replace'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'goto',
                child: Row(
                  children: [
                    Icon(Icons.arrow_forward_rounded, size: 18, color: AppTheme.textSecondary),
                    SizedBox(width: 10),
                    Text('Go to Line'),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'settings',
                child: Row(
                  children: [
                    Icon(Icons.settings_outlined, size: 18, color: AppTheme.primary),
                    SizedBox(width: 10),
                    Text('Editor Settings'),
                  ],
                ),
              ),
              const PopupMenuDivider(),
              const PopupMenuItem(
                value: 'clear_code',
                child: Row(
                  children: [
                    Icon(Icons.clear_all_rounded, size: 18, color: AppTheme.error),
                    SizedBox(width: 10),
                    Text('Clear Code', style: TextStyle(color: AppTheme.error)),
                  ],
                ),
              ),
              const PopupMenuItem(
                value: 'clear_console',
                child: Row(
                  children: [
                    Icon(Icons.cleaning_services_rounded, size: 18, color: AppTheme.warning),
                    SizedBox(width: 10),
                    Text('Clear Output & Diagnostics'),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
      body: Stack(
        children: [
          Column(
            children: [
              if (_diagnostics.isNotEmpty)
                DiagnosticBar(
                  diagnostics: _diagnostics,
                  onTap: (diag) => _jumpToLine(diag.line),
                ),

              Expanded(
                child: Container(
                  color: AppTheme.editorBackground,
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      if (_settings.showLineNumbers)
                        LineNumbers(
                          lineCount: lineCount,
                          fontSize: _settings.fontSize,
                          scrollController: _lineNumScrollController,
                        ),

                      Expanded(
                        child: LayoutBuilder(
                          builder: (context, constraints) {
                            return GestureDetector(
                              behavior: HitTestBehavior.opaque,
                              onTap: () {
                                _focusNode.requestFocus();
                              },
                              child: SingleChildScrollView(
                                controller: _codeScrollController,
                                physics: const AlwaysScrollableScrollPhysics(),
                                child: ConstrainedBox(
                                  constraints: BoxConstraints(
                                    minHeight: constraints.maxHeight,
                                    minWidth: constraints.maxWidth,
                                  ),
                                  child: Padding(
                                    padding: const EdgeInsets.only(
                                      left: 8.0,
                                      top: 8.0,
                                      right: 8.0,
                                      bottom: 160.0,
                                    ),
                                    child: TextField(
                                      focusNode: _focusNode,
                                      controller: _codeController,
                                      maxLines: null,
                                      keyboardType: TextInputType.multiline,
                                      style: _getEditorTextStyle(),
                                      decoration: const InputDecoration(
                                        border: InputBorder.none,
                                        isDense: true,
                                        contentPadding: EdgeInsets.zero,
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                            );
                          },
                        ),
                      ),
                    ],
                  ),
                ),
              ),

              KeyboardAccessoryBar(
                onInsert: (char) {
                  _codeController.insertSnippet(char);
                  _focusNode.requestFocus();
                },
                onTab: () {
                  _codeController.insertSnippet(' ' * _settings.tabSize);
                  _focusNode.requestFocus();
                },
                onIndent: () {
                  _codeController.handleAutoIndent();
                  _focusNode.requestFocus();
                },
                onRun: _runCode,
                onMoveUp: _moveCursorUp,
                onMoveDown: _moveCursorDown,
                onMoveLeft: _moveCursorLeft,
                onMoveRight: _moveCursorRight,
                onMoveHome: _moveCursorHome,
                onMoveEnd: _moveCursorEnd,
                isRunning: _isRunning,
              ),

              // Console toggle bar — draggable and respects bottom system bars
              SafeArea(
                top: false,
                child: GestureDetector(
                  onTap: () {
                    setState(() {
                      _showConsole = !_showConsole;
                    });
                  },
                  onVerticalDragUpdate: (details) {
                    setState(() {
                      _showConsole = true;
                      _consoleHeight = (_consoleHeight - details.delta.dy)
                          .clamp(140.0, MediaQuery.of(context).size.height * 0.75);
                    });
                  },
                  child: Container(
                    color: AppTheme.surfaceLight,
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
                    child: Row(
                      children: [
                        const Icon(Icons.terminal_rounded, size: 15, color: AppTheme.primary),
                        const SizedBox(width: 8),
                        const Text('Console / Output', style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
                        if (_diagnostics.isNotEmpty) ...[
                          const SizedBox(width: 8),
                          Container(
                            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                            decoration: BoxDecoration(
                              color: AppTheme.error.withValues(alpha: 0.2),
                              borderRadius: BorderRadius.circular(8),
                              border: Border.all(color: AppTheme.error.withValues(alpha: 0.5)),
                            ),
                            child: Text(
                              '${_diagnostics.length} issue${_diagnostics.length == 1 ? '' : 's'}',
                              style: const TextStyle(color: AppTheme.error, fontSize: 10, fontWeight: FontWeight.bold),
                            ),
                          ),
                        ],
                        const Spacer(),
                        Container(
                          width: 32,
                          height: 4,
                          margin: const EdgeInsets.symmetric(horizontal: 8),
                          decoration: BoxDecoration(
                            color: AppTheme.border,
                            borderRadius: BorderRadius.circular(2),
                          ),
                        ),
                        const Spacer(),
                        Icon(
                          _showConsole ? Icons.keyboard_arrow_down_rounded : Icons.keyboard_arrow_up_rounded,
                          size: 20,
                        ),
                      ],
                    ),
                  ),
                ),
              ),

              if (_showConsole)
                SizedBox(
                  height: _consoleHeight,
                  child: ConsolePanel(
                    result: _lastResult,
                    isRunning: _isRunning,
                    stdinText: _programStdin,
                    codeText: _codeController.text,
                    onStdinChanged: (val) {
                      setState(() {
                        _programStdin = val;
                      });
                    },
                    onRunInput: (val) {
                      setState(() {
                        _programStdin = val;
                      });
                      _runCode();
                    },
                    onStop: _stopExecution,
                    onClear: () {
                      setState(() {
                        _lastResult = null;
                        _diagnostics = [];
                      });
                    },
                    onDiagnosticTap: (diag) => _jumpToLine(diag.line),
                  ),
                ),
            ],
          ),

          // Autocomplete suggestion overlay
          if (_suggestions.isNotEmpty)
            Positioned(
              left: 54,
              bottom: 90,
              child: AutocompletePopup(
                suggestions: _suggestions,
                onSelect: _applySuggestion,
              ),
            ),
        ],
      ),
    );
  }
}
