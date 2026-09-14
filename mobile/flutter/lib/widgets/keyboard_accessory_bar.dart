import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

class KeyboardAccessoryBar extends StatelessWidget {
  final Function(String) onInsert;
  final VoidCallback onTab;
  final VoidCallback onIndent;
  final VoidCallback onRun;
  final VoidCallback onMoveUp;
  final VoidCallback onMoveDown;
  final VoidCallback onMoveLeft;
  final VoidCallback onMoveRight;
  final VoidCallback? onMoveHome;
  final VoidCallback? onMoveEnd;
  final bool isRunning;

  const KeyboardAccessoryBar({
    super.key,
    required this.onInsert,
    required this.onTab,
    required this.onIndent,
    required this.onRun,
    required this.onMoveUp,
    required this.onMoveDown,
    required this.onMoveLeft,
    required this.onMoveRight,
    this.onMoveHome,
    this.onMoveEnd,
    this.isRunning = false,
  });

  @override
  Widget build(BuildContext context) {
    // Row 1: Run button + most-used symbols
    const row1Symbols = [';', ',', ':', '.', '(', ')', '{', '}', '[', ']', '=', '"', "'"];
    // Row 2: Tab, Indent, Home, End, ← ↑ ↓ →
    return Container(
      color: AppTheme.surfaceLight,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          // ── Row 1 ─────────────────────────────────────────────────
          SizedBox(
            height: 38,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
              children: [
                // Run / Running button
                _RunButton(isRunning: isRunning, onRun: onRun),
                const SizedBox(width: 6),
                ...row1Symbols.map((s) => _SymKey(label: s, onTap: () => onInsert(s))),
              ],
            ),
          ),
          Divider(height: 1, color: AppTheme.border.withValues(alpha: 0.5)),
          // ── Row 2 ─────────────────────────────────────────────────
          SizedBox(
            height: 36,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
              children: [
                _SymKey(label: 'TAB', onTap: onTab),
                _SymKey(label: '⇥ IN', onTap: onIndent),
                if (onMoveHome != null) _SymKey(label: '⇤ HOME', onTap: onMoveHome!),
                if (onMoveEnd != null) _SymKey(label: 'END ⇥', onTap: onMoveEnd!),
                const SizedBox(width: 8),
                _ArrowKey(icon: Icons.keyboard_arrow_left_rounded, onTap: onMoveLeft),
                _ArrowKey(icon: Icons.keyboard_arrow_up_rounded, onTap: onMoveUp),
                _ArrowKey(icon: Icons.keyboard_arrow_down_rounded, onTap: onMoveDown),
                _ArrowKey(icon: Icons.keyboard_arrow_right_rounded, onTap: onMoveRight),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _RunButton extends StatelessWidget {
  final bool isRunning;
  final VoidCallback onRun;
  const _RunButton({required this.isRunning, required this.onRun});

  @override
  Widget build(BuildContext context) {
    return ElevatedButton.icon(
      onPressed: isRunning ? null : onRun,
      style: ElevatedButton.styleFrom(
        backgroundColor: AppTheme.secondary,
        foregroundColor: Colors.black,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 0),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        minimumSize: const Size(0, 30),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      icon: isRunning
          ? const SizedBox(
              width: 12,
              height: 12,
              child: CircularProgressIndicator(strokeWidth: 2, color: Colors.black),
            )
          : const Icon(Icons.play_arrow_rounded, size: 16),
      label: Text(isRunning ? 'Running' : 'Run',
          style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
    );
  }
}

class _SymKey extends StatelessWidget {
  final String label;
  final VoidCallback onTap;
  const _SymKey({required this.label, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 2),
      child: Material(
        color: AppTheme.surface,
        borderRadius: BorderRadius.circular(5),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(5),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 4),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              border: Border.all(color: AppTheme.border, width: 1),
              borderRadius: BorderRadius.circular(5),
            ),
            child: Text(
              label,
              style: const TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 12,
                fontWeight: FontWeight.w600,
                fontFamily: 'monospace',
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ArrowKey extends StatelessWidget {
  final IconData icon;
  final VoidCallback onTap;
  const _ArrowKey({required this.icon, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 2),
      child: Material(
        color: AppTheme.primary.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(5),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(5),
          child: Container(
            width: 36,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              border: Border.all(color: AppTheme.primary.withValues(alpha: 0.3), width: 1),
              borderRadius: BorderRadius.circular(5),
            ),
            child: Icon(icon, size: 20, color: AppTheme.primary),
          ),
        ),
      ),
    );
  }
}
