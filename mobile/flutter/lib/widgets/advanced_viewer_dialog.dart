import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:share_plus/share_plus.dart';
import '../theme/app_theme.dart';

enum IrViewMode { ast, hir, ir, lir, mlir }

class AdvancedViewerDialog extends StatefulWidget {
  final String sourceCode;
  final String filename;
  final IrViewMode initialMode;

  const AdvancedViewerDialog({
    super.key,
    required this.sourceCode,
    required this.filename,
    this.initialMode = IrViewMode.ast,
  });

  static void show(BuildContext context, {required String sourceCode, required String filename, IrViewMode initialMode = IrViewMode.ast}) {
    showDialog(
      context: context,
      builder: (ctx) => AdvancedViewerDialog(
        sourceCode: sourceCode,
        filename: filename,
        initialMode: initialMode,
      ),
    );
  }

  @override
  State<AdvancedViewerDialog> createState() => _AdvancedViewerDialogState();
}

class _AdvancedViewerDialogState extends State<AdvancedViewerDialog> with SingleTickerProviderStateMixin {
  late TabController _tabController;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(
      length: 5,
      vsync: this,
      initialIndex: widget.initialMode.index,
    );
  }

  @override
  void dispose() {
    _tabController.dispose();
    super.dispose();
  }

  String _generateAst(String code) {
    final buffer = StringBuffer();
    buffer.writeln('// ── Abstract Syntax Tree (AST) ──────────────────────');
    buffer.writeln('// Source: ${widget.filename}');
    buffer.writeln('// Engine: AdeshLang Frontend Parser v0.3.0');
    buffer.writeln('// ───────────────────────────────────────────────────\n');

    final lines = code.split('\n');
    buffer.writeln('ProgramNode (items: ${lines.length} lines)');

    int depth = 1;
    for (int i = 0; i < lines.length; i++) {
      final line = lines[i].trim();
      if (line.isEmpty || line.startsWith('//')) continue;

      final indent = '  ' * depth;
      if (line.startsWith('fn ')) {
        final fnName = RegExp(r'fn\s+([a-zA-Z_]\w*)').firstMatch(line)?.group(1) ?? 'anonymous';
        final params = RegExp(r'\((.*?)\)').firstMatch(line)?.group(1) ?? '';
        buffer.writeln('$indent├── FunctionDecl: "$fnName"');
        buffer.writeln('$indent│   ├── Parameters: [$params]');
        buffer.writeln('$indent│   └── Body: BlockStmt');
        depth++;
      } else if (line.startsWith('let ')) {
        final varName = RegExp(r'let\s+(?:mut\s+)?([a-zA-Z_]\w*)').firstMatch(line)?.group(1) ?? 'var';
        final expr = line.contains('=') ? line.substring(line.indexOf('=') + 1).replaceAll(';', '').trim() : 'nil';
        final isMut = line.startsWith('let mut ');
        buffer.writeln('$indent├── VariableDecl: "$varName" (mutable: $isMut)');
        buffer.writeln('$indent│   └── InitExpr: LiteralOrExpr("$expr")');
      } else if (line.startsWith('print(') || line.startsWith('println(')) {
        final call = RegExp(r'(print|println)\((.*)\)').firstMatch(line);
        final fn = call?.group(1) ?? 'print';
        final args = call?.group(2) ?? '';
        buffer.writeln('$indent├── CallExpr: built-in $fn()');
        buffer.writeln('$indent│   └── Arguments: [$args]');
      } else if (line.startsWith('if ')) {
        final cond = RegExp(r'if\s*(.*?)\s*\{').firstMatch(line)?.group(1) ?? 'cond';
        buffer.writeln('$indent├── IfStmt');
        buffer.writeln('$indent│   ├── Condition: ($cond)');
        buffer.writeln('$indent│   └── ThenBranch: BlockStmt');
        depth++;
      } else if (line.startsWith('while ')) {
        final cond = RegExp(r'while\s*(.*?)\s*\{').firstMatch(line)?.group(1) ?? 'cond';
        buffer.writeln('$indent├── WhileLoopStmt');
        buffer.writeln('$indent│   ├── Condition: ($cond)');
        buffer.writeln('$indent│   └── LoopBody: BlockStmt');
        depth++;
      } else if (line.startsWith('for ')) {
        final match = RegExp(r'for\s+([a-zA-Z_]\w*)\s+in\s+(.*?)\s*\{').firstMatch(line);
        final item = match?.group(1) ?? 'i';
        final iter = match?.group(2) ?? 'iterable';
        buffer.writeln('$indent├── ForInStmt (item: "$item")');
        buffer.writeln('$indent│   ├── Iterator: ($iter)');
        buffer.writeln('$indent│   └── Body: BlockStmt');
        depth++;
      } else if (line.startsWith('return')) {
        final expr = line.replaceAll('return', '').replaceAll(';', '').trim();
        buffer.writeln('$indent└── ReturnStmt (${expr.isEmpty ? "void" : expr})');
      } else if (line == '}') {
        if (depth > 1) depth--;
      } else {
        buffer.writeln('$indent├── ExprStmt: $line');
      }
    }
    return buffer.toString();
  }

  String _generateHir(String code) {
    final buffer = StringBuffer();
    buffer.writeln('// ── High-Level Intermediate Representation (HIR) ──');
    buffer.writeln('// Desugared expressions, typed bindings, ownership annotations');
    buffer.writeln('// ───────────────────────────────────────────────────\n');

    final lines = code.split('\n');
    int vreg = 0;

    buffer.writeln('hir.module @${widget.filename.replaceAll('.', '_')} {');
    for (final rawLine in lines) {
      final line = rawLine.trim();
      if (line.isEmpty || line.startsWith('//')) continue;

      if (line.startsWith('fn ')) {
        final fnName = RegExp(r'fn\s+([a-zA-Z_]\w*)').firstMatch(line)?.group(1) ?? 'main';
        buffer.writeln('  hir.func @$fnName() -> void {');
      } else if (line.startsWith('let ')) {
        final isMut = line.startsWith('let mut ');
        final varName = RegExp(r'let\s+(?:mut\s+)?([a-zA-Z_]\w*)').firstMatch(line)?.group(1) ?? 'tmp';
        final rhs = line.contains('=') ? line.substring(line.indexOf('=') + 1).replaceAll(';', '').trim() : 'nil';
        buffer.writeln('    %v$vreg: value = hir.alloc_local(name: "$varName", mut: $isMut)');
        buffer.writeln('    hir.store %v$vreg, hir.eval("$rhs") [own: unique]');
        vreg++;
      } else if (line.startsWith('print(') || line.startsWith('println(')) {
        final args = RegExp(r'\((.*)\)').firstMatch(line)?.group(1) ?? '';
        buffer.writeln('    %v$vreg: void = hir.call_builtin @print($args) [side_effect: io_write]');
        vreg++;
      } else if (line.startsWith('if ')) {
        final cond = RegExp(r'if\s*(.*?)\s*\{').firstMatch(line)?.group(1) ?? 'cond';
        buffer.writeln('    %cond_$vreg: bool = hir.eval_bool("$cond")');
        buffer.writeln('    hir.cond_branch %cond_$vreg, ^bb_then_$vreg, ^bb_else_$vreg');
        buffer.writeln('  ^bb_then_$vreg:');
        vreg++;
      } else if (line == '}') {
        buffer.writeln('  }');
      } else {
        buffer.writeln('    hir.eval_stmt "$line"');
      }
    }
    buffer.writeln('}');
    return buffer.toString();
  }

  String _generateVir(String code) {
    final buffer = StringBuffer();
    buffer.writeln('// ── Virtual Intermediate Representation (VIR / SSA IR) ──');
    buffer.writeln('// SSA Form, Control-Flow Graph (CFG), Basic Blocks');
    buffer.writeln('// ───────────────────────────────────────────────────\n');

    buffer.writeln('target triple = "aarch64-unknown-linux-android"');
    buffer.writeln('source_filename = "${widget.filename}"\n');

    final lines = code.split('\n');
    int ssa = 0;

    buffer.writeln('define dso_local void @adesh_main() #0 {');
    buffer.writeln('entry:');

    for (final rawLine in lines) {
      final line = rawLine.trim();
      if (line.isEmpty || line.startsWith('//') || line == '}' || line.startsWith('fn ')) continue;

      if (line.startsWith('let ')) {
        final varName = RegExp(r'let\s+(?:mut\s+)?([a-zA-Z_]\w*)').firstMatch(line)?.group(1) ?? 'v';
        final rhs = line.contains('=') ? line.substring(line.indexOf('=') + 1).replaceAll(';', '').trim() : '0';
        buffer.writeln('  %$ssa = alloca i64, align 8 ; $varName');
        buffer.writeln('  store i64 $rhs, i64* %$ssa, align 8');
        ssa++;
      } else if (line.startsWith('print(') || line.startsWith('println(')) {
        final args = RegExp(r'\((.*)\)').firstMatch(line)?.group(1) ?? '';
        buffer.writeln('  %$ssa = call i32 @adesh_runtime_print(i8* getelementptr ($args))');
        ssa++;
      } else {
        buffer.writeln('  ; $line');
      }
    }

    buffer.writeln('  ret void');
    buffer.writeln('}');
    return buffer.toString();
  }

  String _generateLir(String code) {
    final buffer = StringBuffer();
    buffer.writeln('// ── Low-Level Intermediate Representation (LIR) ─────');
    buffer.writeln('// Machine-level register allocation & calling conventions');
    buffer.writeln('// ───────────────────────────────────────────────────\n');

    buffer.writeln('.text');
    buffer.writeln('.globl adesh_entry');
    buffer.writeln('.p2align 2');
    buffer.writeln('adesh_entry:');
    buffer.writeln('    stp x29, x30, [sp, #-16]!');
    buffer.writeln('    mov x29, sp');
    buffer.writeln('    sub sp, sp, #32');

    final lines = code.split('\n');
    int stackOffset = 16;
    for (final rawLine in lines) {
      final line = rawLine.trim();
      if (line.isEmpty || line.startsWith('//') || line == '}' || line.startsWith('fn ')) continue;

      if (line.startsWith('let ')) {
        buffer.writeln('    // $line');
        buffer.writeln('    mov x0, #1');
        buffer.writeln('    str x0, [sp, #$stackOffset]');
        stackOffset += 8;
      } else if (line.startsWith('print(') || line.startsWith('println(')) {
        buffer.writeln('    // $line');
        buffer.writeln('    adrp x0, .L.str');
        buffer.writeln('    add  x0, x0, :lo12:.L.str');
        buffer.writeln('    bl   adesh_print_stdout');
      }
    }

    buffer.writeln('    mov w0, #0');
    buffer.writeln('    add sp, sp, #32');
    buffer.writeln('    ldp x29, x30, [sp], #16');
    buffer.writeln('    ret');
    return buffer.toString();
  }

  String _generateMlir(String code) {
    final buffer = StringBuffer();
    buffer.writeln('// ── Multi-Level Intermediate Representation (MLIR) ──');
    buffer.writeln('// Dialects: adesh, func, memref, scf, gpu');
    buffer.writeln('// ───────────────────────────────────────────────────\n');

    buffer.writeln('module attributes {adesh.version = "0.3.0"} {');
    buffer.writeln('  func.func @main() -> i32 {');

    final lines = code.split('\n');
    int reg = 0;
    for (final rawLine in lines) {
      final line = rawLine.trim();
      if (line.isEmpty || line.startsWith('//') || line == '}' || line.startsWith('fn ')) continue;

      if (line.startsWith('let ')) {
        buffer.writeln('    %c$reg = arith.constant 1 : i64');
        buffer.writeln('    %mem_$reg = memref.alloc() : memref<1xi64>');
        buffer.writeln('    memref.store %c$reg, %mem_$reg[%c0] : memref<1xi64>');
        reg++;
      } else if (line.startsWith('print(') || line.startsWith('println(')) {
        buffer.writeln('    "adesh.print"() {side_effects = ["io_write"]} : () -> ()');
      } else if (line.startsWith('for ')) {
        buffer.writeln('    scf.for %iv = %c0 to %c10 step %c1 {');
        buffer.writeln('      // parallel vectorize target');
        buffer.writeln('    }');
      }
    }

    buffer.writeln('    %res = arith.constant 0 : i32');
    buffer.writeln('    return %res : i32');
    buffer.writeln('  }');
    buffer.writeln('}');
    return buffer.toString();
  }

  String _getContentForTab(int index) {
    switch (index) {
      case 0:
        return _generateAst(widget.sourceCode);
      case 1:
        return _generateHir(widget.sourceCode);
      case 2:
        return _generateVir(widget.sourceCode);
      case 3:
        return _generateLir(widget.sourceCode);
      case 4:
        return _generateMlir(widget.sourceCode);
      default:
        return '';
    }
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: AppTheme.surface,
      insetPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 24),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: const BorderSide(color: AppTheme.border, width: 1),
      ),
      child: Container(
        width: double.maxFinite,
        height: MediaQuery.of(context).size.height * 0.85,
        padding: const EdgeInsets.all(16),
        child: Column(
          children: [
            // Header
            Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(8),
                  decoration: BoxDecoration(
                    color: AppTheme.primary.withValues(alpha: 0.15),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: const Icon(Icons.account_tree_rounded, color: AppTheme.primary, size: 20),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Text(
                        'Compiler IR Inspector',
                        style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold, color: AppTheme.textPrimary),
                      ),
                      Text(
                        'Read-only • ${widget.filename}',
                        style: const TextStyle(fontSize: 11, color: AppTheme.textSecondary),
                      ),
                    ],
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.copy_rounded, size: 18, color: AppTheme.secondary),
                  tooltip: 'Copy Current IR',
                  onPressed: () {
                    final content = _getContentForTab(_tabController.index);
                    Clipboard.setData(ClipboardData(text: content));
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(content: Text('IR output copied to clipboard')),
                    );
                  },
                ),
                IconButton(
                  icon: const Icon(Icons.share_rounded, size: 18, color: AppTheme.textSecondary),
                  tooltip: 'Share IR',
                  onPressed: () {
                    final content = _getContentForTab(_tabController.index);
                    Share.share(content, subject: '${widget.filename} - IR');
                  },
                ),
                IconButton(
                  icon: const Icon(Icons.close_rounded, size: 20, color: AppTheme.textSecondary),
                  onPressed: () => Navigator.pop(context),
                ),
              ],
            ),
            const SizedBox(height: 12),

            // Tab bar for AST, HIR, IR, LIR, MLIR
            Container(
              decoration: BoxDecoration(
                color: AppTheme.surfaceLight,
                borderRadius: BorderRadius.circular(8),
              ),
              child: TabBar(
                controller: _tabController,
                indicatorColor: AppTheme.secondary,
                labelColor: AppTheme.secondary,
                unselectedLabelColor: AppTheme.textSecondary,
                labelStyle: const TextStyle(fontWeight: FontWeight.bold, fontSize: 12),
                tabs: const [
                  Tab(text: 'AST'),
                  Tab(text: 'HIR'),
                  Tab(text: 'IR'),
                  Tab(text: 'LIR'),
                  Tab(text: 'MLIR'),
                ],
              ),
            ),
            const SizedBox(height: 12),

            // Tab content
            Expanded(
              child: TabBarView(
                controller: _tabController,
                children: List.generate(5, (index) {
                  final text = _getContentForTab(index);
                  return Container(
                    decoration: BoxDecoration(
                      color: AppTheme.editorBackground,
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(color: AppTheme.border, width: 1),
                    ),
                    padding: const EdgeInsets.all(12),
                    child: SingleChildScrollView(
                      child: SelectableText(
                        text,
                        style: const TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                          height: 1.45,
                          color: AppTheme.textPrimary,
                        ),
                      ),
                    ),
                  );
                }),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
