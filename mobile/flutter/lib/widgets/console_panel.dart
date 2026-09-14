import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:share_plus/share_plus.dart';

import '../bridge/adesh_bridge_types.dart';
import '../theme/app_theme.dart';
import 'ansi_text.dart';

class ConsolePanel extends StatefulWidget {
  final ExecutionResult? result;
  final bool isRunning;
  final bool focusErrors;
  final String stdinText;
  final String? codeText;
  final Function(String)? onStdinChanged;
  final Function(String)? onRunInput;
  final VoidCallback? onClear;
  final VoidCallback? onStop;
  final Function(Diagnostic)? onDiagnosticTap;

  const ConsolePanel({
    super.key,
    this.result,
    this.isRunning = false,
    this.focusErrors = false,
    this.stdinText = '',
    this.codeText,
    this.onStdinChanged,
    this.onRunInput,
    this.onClear,
    this.onStop,
    this.onDiagnosticTap,
  });

  @override
  State<ConsolePanel> createState() => _ConsolePanelState();
}

class _ConsolePanelState extends State<ConsolePanel> with SingleTickerProviderStateMixin {
  late TabController _tabController;
  late TextEditingController _stdinController;
  final TextEditingController _quickInputController = TextEditingController();
  final FocusNode _inputFocusNode = FocusNode();

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 4, vsync: this);
    _stdinController = TextEditingController(text: widget.stdinText);
    _autoFocusTab();
  }

  @override
  void didUpdateWidget(ConsolePanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.stdinText != _stdinController.text && widget.stdinText != oldWidget.stdinText) {
      _stdinController.text = widget.stdinText;
    }
    if (widget.result != oldWidget.result) {
      _autoFocusTab();
    }
  }

  void _autoFocusTab() {
    final result = widget.result;
    if (result == null) return;
    if (result.diagnostics.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _tabController.animateTo(3);
      });
    } else if (result.stderr.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _tabController.animateTo(2);
      });
    } else if (result.stdout.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _tabController.animateTo(0);
      });
    }
  }

  @override
  void dispose() {
    _tabController.dispose();
    _stdinController.dispose();
    _quickInputController.dispose();
    _inputFocusNode.dispose();
    super.dispose();
  }

  void _submitInput(String text) {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return;

    final updatedStdin = _stdinController.text.isEmpty
        ? trimmed
        : '${_stdinController.text}\n$trimmed';

    _stdinController.text = updatedStdin;
    widget.onStdinChanged?.call(updatedStdin);
    _quickInputController.clear();
    widget.onRunInput?.call(updatedStdin);
  }

  bool _codeHasInputCall() {
    final code = widget.codeText ?? '';
    return RegExp(r'\binput\s*\(|\bInput\.').hasMatch(code);
  }

  @override
  Widget build(BuildContext context) {
    final result = widget.result;

    return Container(
      decoration: const BoxDecoration(
        color: AppTheme.surface,
        border: Border(top: BorderSide(color: AppTheme.border, width: 1)),
      ),
      child: Column(
        children: [
          // ── Header Bar ─────────────────────────────────────────────
          Container(
            height: 40,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            color: AppTheme.surfaceLight,
            child: Row(
              children: [
                _buildStatusBadge(result),
                const SizedBox(width: 8),

                if (result != null && !widget.isRunning) ...[
                  Text(
                    '${result.executionTimeMs.toStringAsFixed(2)} ms',
                    style: const TextStyle(
                      color: AppTheme.textSecondary,
                      fontSize: 12,
                      fontFamily: 'monospace',
                    ),
                  ),
                ],

                const Spacer(),

                if (widget.isRunning) ...[
                  ElevatedButton.icon(
                    onPressed: widget.onStop,
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.error,
                      foregroundColor: Colors.white,
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                      minimumSize: const Size(0, 28),
                    ),
                    icon: const Icon(Icons.stop_rounded, size: 14),
                    label: const Text('Stop', style: TextStyle(fontSize: 11)),
                  ),
                  const SizedBox(width: 6),
                ],

                IconButton(
                  icon: const Icon(Icons.copy_rounded, size: 16, color: AppTheme.textSecondary),
                  tooltip: 'Copy Output',
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                  onPressed: result != null
                      ? () {
                          final raw = '${result.stdout}\n${result.stderr}'.trim();
                          final text = AnsiTextSpanParser.strip(raw);
                          Clipboard.setData(ClipboardData(text: text));
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(content: Text('Console output copied to clipboard')),
                          );
                        }
                      : null,
                ),
                IconButton(
                  icon: const Icon(Icons.share_rounded, size: 16, color: AppTheme.textSecondary),
                  tooltip: 'Share Output',
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                  onPressed: result != null
                      ? () {
                          final raw = '${result.stdout}\n${result.stderr}'.trim();
                          final text = AnsiTextSpanParser.strip(raw);
                          if (text.isNotEmpty) {
                            Share.share(text, subject: 'AdeshLang Console Output');
                          }
                        }
                      : null,
                ),
                IconButton(
                  icon: const Icon(Icons.delete_outline_rounded, size: 16, color: AppTheme.textSecondary),
                  tooltip: 'Clear Console',
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
                  onPressed: widget.onClear,
                ),
              ],
            ),
          ),

          // ── Tab Header ─────────────────────────────────────────────
          Container(
            height: 34,
            color: AppTheme.surfaceLight,
            child: TabBar(
              controller: _tabController,
              indicatorColor: AppTheme.primary,
              labelColor: AppTheme.primary,
              unselectedLabelColor: AppTheme.textSecondary,
              labelStyle: const TextStyle(fontSize: 11, fontWeight: FontWeight.bold),
              tabs: [
                Tab(
                  text: 'OUTPUT${result != null && result.stdout.isNotEmpty ? ' •' : ''}',
                ),
                Tab(
                  text: 'INPUT (stdin)${_stdinController.text.isNotEmpty ? ' •' : ''}',
                ),
                Tab(
                  text: 'ERRORS${result != null && result.stderr.isNotEmpty ? ' !' : ''}',
                ),
                Tab(
                  text: 'DIAGNOSTICS${result != null && result.diagnostics.isNotEmpty ? ' (${result.diagnostics.length})' : ''}',
                ),
              ],
            ),
          ),

          // ── Tab Views ──────────────────────────────────────────────
          Expanded(
            child: TabBarView(
              controller: _tabController,
              children: [
                _buildOutputTab(result?.stdout ?? ''),
                _buildInputTab(),
                _buildErrorTab(result?.stderr ?? ''),
                _buildDiagnosticsTab(result?.diagnostics ?? []),
              ],
            ),
          ),

          // ── Notch-Aware Quick Interactive Input Bar ────────────────
          SafeArea(
            top: false,
            child: Container(
              padding: const EdgeInsets.fromLTRB(10, 6, 10, 10),
              decoration: BoxDecoration(
                color: AppTheme.surfaceLight,
                border: Border(
                  top: BorderSide(color: AppTheme.border.withValues(alpha: 0.6), width: 1),
                ),
              ),
              child: Row(
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
                    decoration: BoxDecoration(
                      color: AppTheme.primary.withValues(alpha: 0.15),
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: const Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(Icons.keyboard_outlined, size: 14, color: AppTheme.primary),
                        SizedBox(width: 4),
                        Text('stdin', style: TextStyle(fontSize: 10, fontWeight: FontWeight.bold, color: AppTheme.primary)),
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 10),
                      decoration: BoxDecoration(
                        color: AppTheme.editorBackground,
                        borderRadius: BorderRadius.circular(6),
                        border: Border.all(color: AppTheme.border),
                      ),
                      child: TextField(
                        controller: _quickInputController,
                        focusNode: _inputFocusNode,
                        style: const TextStyle(fontSize: 12, fontFamily: 'monospace', color: AppTheme.textPrimary),
                        decoration: const InputDecoration(
                          hintText: 'Enter input for input() and press Send...',
                          hintStyle: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
                          isDense: true,
                          contentPadding: EdgeInsets.symmetric(vertical: 8),
                          border: InputBorder.none,
                        ),
                        onSubmitted: (val) => _submitInput(val),
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  ElevatedButton.icon(
                    onPressed: () => _submitInput(_quickInputController.text),
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppTheme.secondary,
                      foregroundColor: Colors.black,
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 0),
                      minimumSize: const Size(0, 34),
                      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
                    ),
                    icon: const Icon(Icons.send_rounded, size: 13),
                    label: const Text('Send', style: TextStyle(fontSize: 11, fontWeight: FontWeight.bold)),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildStatusBadge(ExecutionResult? result) {
    if (widget.isRunning) {
      return const Row(
        children: [
          SizedBox(
            width: 12,
            height: 12,
            child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.primary),
          ),
          SizedBox(width: 6),
          Text(
            'Running...',
            style: TextStyle(color: AppTheme.primary, fontWeight: FontWeight.bold, fontSize: 12),
          ),
        ],
      );
    }

    if (result == null) {
      return const Text(
        'Console Ready',
        style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
      );
    }

    switch (result.status) {
      case ExecutionStatus.completed:
        return const Row(
          children: [
            Icon(Icons.check_circle_rounded, color: AppTheme.secondary, size: 14),
            SizedBox(width: 4),
            Text(
              'Finished',
              style: TextStyle(color: AppTheme.secondary, fontWeight: FontWeight.bold, fontSize: 12),
            ),
          ],
        );
      case ExecutionStatus.failed:
        return const Row(
          children: [
            Icon(Icons.error_rounded, color: AppTheme.error, size: 14),
            SizedBox(width: 4),
            Text(
              'Failed',
              style: TextStyle(color: AppTheme.error, fontWeight: FontWeight.bold, fontSize: 12),
            ),
          ],
        );
      case ExecutionStatus.cancelled:
        return const Row(
          children: [
            Icon(Icons.cancel_rounded, color: AppTheme.warning, size: 14),
            SizedBox(width: 4),
            Text(
              'Cancelled',
              style: TextStyle(color: AppTheme.warning, fontWeight: FontWeight.bold, fontSize: 12),
            ),
          ],
        );
      default:
        return const Text('Idle', style: TextStyle(color: AppTheme.textSecondary, fontSize: 12));
    }
  }

  Widget _buildOutputTab(String stdout) {
    final hasInput = _codeHasInputCall();
    final stdinLines = _stdinController.text.trim();

    return Container(
      color: AppTheme.background,
      padding: const EdgeInsets.all(12),
      width: double.infinity,
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // If inputs were provided, display them cleanly in output log
            if (stdinLines.isNotEmpty) ...[
              Container(
                margin: const EdgeInsets.only(bottom: 8),
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                decoration: BoxDecoration(
                  color: AppTheme.surfaceLight,
                  borderRadius: BorderRadius.circular(4),
                  border: Border.all(color: AppTheme.border),
                ),
                child: Row(
                  children: [
                    const Icon(Icons.input_rounded, size: 12, color: AppTheme.secondary),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Text(
                        'Input supplied: ${stdinLines.replaceAll('\n', ' ↵ ')}',
                        style: const TextStyle(
                          fontSize: 11,
                          fontFamily: 'monospace',
                          color: AppTheme.secondary,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],

            // If the code uses input() and no input was provided yet, show a prominent inline prompt
            if (hasInput && stdinLines.isEmpty && stdout.trim().isEmpty) ...[
              Container(
                margin: const EdgeInsets.only(bottom: 12),
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: AppTheme.surface,
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: AppTheme.primary.withValues(alpha: 0.5)),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Row(
                      children: [
                        Icon(Icons.keyboard_alt_outlined, color: AppTheme.primary, size: 18),
                        SizedBox(width: 8),
                        Text(
                          'Interactive Program Input Required',
                          style: TextStyle(fontSize: 12, fontWeight: FontWeight.bold, color: AppTheme.textPrimary),
                        ),
                      ],
                    ),
                    const SizedBox(height: 6),
                    const Text(
                      'Your program calls input(). Enter your input below to execute with your values:',
                      style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
                    ),
                    const SizedBox(height: 10),
                    Row(
                      children: [
                        Expanded(
                          child: Container(
                            height: 36,
                            padding: const EdgeInsets.symmetric(horizontal: 10),
                            decoration: BoxDecoration(
                              color: AppTheme.editorBackground,
                              borderRadius: BorderRadius.circular(6),
                              border: Border.all(color: AppTheme.border),
                            ),
                            child: TextField(
                              style: const TextStyle(fontFamily: 'monospace', fontSize: 12, color: AppTheme.textPrimary),
                              decoration: const InputDecoration(
                                hintText: 'Enter input value...',
                                hintStyle: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
                                isDense: true,
                                border: InputBorder.none,
                                contentPadding: EdgeInsets.symmetric(vertical: 8),
                              ),
                              onSubmitted: (v) => _submitInput(v),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ],

            // Main terminal output
            if (stdout.trim().isNotEmpty) ...[
              AnsiText(
                stdout,
                style: const TextStyle(
                  fontFamily: 'monospace',
                  color: AppTheme.textPrimary,
                  fontSize: 12,
                  height: 1.45,
                ),
              ),
            ] else if (!hasInput || stdinLines.isNotEmpty) ...[
              const Text(
                'No stdout output produced.',
                style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildInputTab() {
    return Container(
      color: AppTheme.background,
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Program Standard Input (stdin):',
            style: TextStyle(fontWeight: FontWeight.bold, fontSize: 12, color: AppTheme.textPrimary),
          ),
          const SizedBox(height: 4),
          const Text(
            'Enter inputs line by line. When code calls input(), lines are supplied sequentially.',
            style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
          ),
          const SizedBox(height: 8),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: AppTheme.editorBackground,
                borderRadius: BorderRadius.circular(6),
                border: Border.all(color: AppTheme.border),
              ),
              padding: const EdgeInsets.all(8),
              child: TextField(
                controller: _stdinController,
                maxLines: null,
                keyboardType: TextInputType.multiline,
                style: const TextStyle(fontFamily: 'monospace', fontSize: 12, color: AppTheme.textPrimary),
                decoration: const InputDecoration(
                  hintText: 'line 1\nline 2\n...',
                  hintStyle: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
                  border: InputBorder.none,
                  isDense: true,
                ),
                onChanged: (val) => widget.onStdinChanged?.call(val),
              ),
            ),
          ),
          const SizedBox(height: 6),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              TextButton.icon(
                icon: const Icon(Icons.play_arrow_rounded, size: 16, color: AppTheme.secondary),
                label: const Text('Run with Current Input', style: TextStyle(fontSize: 11, color: AppTheme.secondary)),
                onPressed: () {
                  widget.onRunInput?.call(_stdinController.text);
                },
              ),
              TextButton.icon(
                icon: const Icon(Icons.clear_rounded, size: 14),
                label: const Text('Clear Input', style: TextStyle(fontSize: 11)),
                onPressed: () {
                  _stdinController.clear();
                  widget.onStdinChanged?.call('');
                },
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildErrorTab(String stderr) {
    if (stderr.trim().isEmpty) {
      return const Center(
        child: Text(
          'No runtime errors or warnings',
          style: TextStyle(color: AppTheme.secondary, fontSize: 12),
        ),
      );
    }

    return Container(
      color: AppTheme.background,
      padding: const EdgeInsets.all(12),
      width: double.infinity,
      child: SingleChildScrollView(
        child: AnsiText(
          stderr,
          style: const TextStyle(
            fontFamily: 'monospace',
            color: AppTheme.error,
            fontSize: 12,
            height: 1.4,
          ),
        ),
      ),
    );
  }

  Widget _buildDiagnosticsTab(List<Diagnostic> diagnostics) {
    if (diagnostics.isEmpty) {
      return const Center(
        child: Text(
          'No diagnostics found. Check passed!',
          style: TextStyle(color: AppTheme.secondary, fontSize: 12),
        ),
      );
    }

    return Container(
      color: AppTheme.background,
      child: ListView.separated(
        padding: const EdgeInsets.all(8),
        itemCount: diagnostics.length,
        separatorBuilder: (context, index) => const SizedBox(height: 6),
        itemBuilder: (context, index) {
          final diag = diagnostics[index];
          final color = diag.severity == DiagnosticSeverity.error
              ? AppTheme.error
              : diag.severity == DiagnosticSeverity.warning
                  ? AppTheme.warning
                  : AppTheme.primary;

          return Card(
            color: AppTheme.surface,
            margin: EdgeInsets.zero,
            child: ListTile(
              dense: true,
              leading: Icon(
                diag.severity == DiagnosticSeverity.error ? Icons.error_outline_rounded : Icons.warning_amber_rounded,
                color: color,
                size: 20,
              ),
              title: Text(
                diag.message,
                style: TextStyle(fontSize: 12, fontWeight: FontWeight.w600, color: color),
              ),
              subtitle: Text(
                'Line ${diag.line}, Column ${diag.column}${diag.code != null ? ' • [${diag.code}]' : ''}',
                style: const TextStyle(fontSize: 11, color: AppTheme.textSecondary, fontFamily: 'monospace'),
              ),
              trailing: const Icon(Icons.arrow_forward_ios_rounded, size: 12, color: AppTheme.textSecondary),
              onTap: () => widget.onDiagnosticTap?.call(diag),
            ),
          );
        },
      ),
    );
  }
}
