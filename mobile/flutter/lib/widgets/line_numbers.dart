import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

class LineNumbers extends StatelessWidget {
  final int lineCount;
  final double fontSize;
  final double lineHeight;
  final ScrollController scrollController;

  const LineNumbers({
    super.key,
    required this.lineCount,
    this.fontSize = 14.0,
    this.lineHeight = 1.4,
    required this.scrollController,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 44,
      color: AppTheme.surface,
      child: SingleChildScrollView(
        controller: scrollController,
        physics: const NeverScrollableScrollPhysics(),
        child: Padding(
          padding: const EdgeInsets.only(top: 8.0, bottom: 160.0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: List.generate(
              lineCount.clamp(1, 99999),
              (index) => Container(
                height: fontSize * lineHeight,
                padding: const EdgeInsets.only(right: 8),
                alignment: Alignment.centerRight,
                child: Text(
                  '${index + 1}',
                  style: TextStyle(
                    color: AppTheme.lineNumbers,
                    fontSize: fontSize,
                    fontFamily: 'monospace',
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
