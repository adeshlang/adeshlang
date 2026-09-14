import 'package:flutter/material.dart';

import '../bridge/adesh_bridge_types.dart';
import '../theme/app_theme.dart';

class DiagnosticBar extends StatelessWidget {
  final List<Diagnostic> diagnostics;
  final Function(Diagnostic) onTap;

  const DiagnosticBar({
    super.key,
    required this.diagnostics,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    if (diagnostics.isEmpty) {
      return const SizedBox.shrink();
    }

    final firstDiag = diagnostics.first;
    final isError = firstDiag.severity == DiagnosticSeverity.error;
    final color = isError ? AppTheme.error : AppTheme.warning;

    return GestureDetector(
      onTap: () => onTap(firstDiag),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        color: color.withValues(alpha: 0.15),
        child: Row(
          children: [
            Icon(
              isError ? Icons.error_outline_rounded : Icons.warning_amber_rounded,
              color: color,
              size: 16,
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                'Line ${firstDiag.line}, Column ${firstDiag.column}: ${firstDiag.message}',
                style: TextStyle(
                  color: color,
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            if (diagnostics.length > 1) ...[
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: color.withValues(alpha: 0.3),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Text(
                  '+${diagnostics.length - 1} more',
                  style: TextStyle(color: color, fontSize: 10, fontWeight: FontWeight.bold),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
