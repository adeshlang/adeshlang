import 'package:flutter/material.dart';
import '../theme/app_theme.dart';

class LearnScreen extends StatelessWidget {
  const LearnScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Learn AdeshLang'),
      ),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          _buildTopicCard(
            title: '1. Functions (`fn`)',
            subtitle: 'Function declaration syntax',
            codeSnippet: 'fn add(a, b) {\n    return a + b;\n}\n\nfn greet(name) {\n    print("Hello,", name);\n}',
            explanation:
                'Functions are defined using `fn`. They support explicit parameters and return values. The `fn main()` entry point runs automatically — no need to call `main()` manually.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '2. Variables (`let`)',
            subtitle: 'Variable bindings & inference',
            codeSnippet: 'let name = "AdeshLang";\nlet count: i32 = 42;\nlet mut score = 0;\nscore = score + 10;',
            explanation:
                'Variables are immutable by default. Use `let mut` to make a variable mutable. Types are inferred automatically, but can be annotated with `: Type`.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '3. Printing (`print`)',
            subtitle: 'Outputting values to stdout',
            codeSnippet: 'print("Hello,", "AdeshLang!", 2026);\nprint("Score:", 42, true);',
            explanation:
                'The built-in `print` function formats and outputs zero or more values separated by space.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '4. Control Flow (`if`, `else`)',
            subtitle: 'Conditionals',
            codeSnippet:
                'let x = 15;\nif (x > 10) {\n    print("Large");\n} else if (x == 5) {\n    print("Five");\n} else {\n    print("Small");\n}',
            explanation:
                'Standard block-scoped `if`, `else if`, `else`. Conditions must evaluate to a boolean.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '5. Loops (`while`, `for`)',
            subtitle: 'Iteration and repetition',
            codeSnippet: 'let mut i = 0;\nwhile (i < 5) {\n    print(i);\n    i = i + 1;\n}\n\nfor n in [1, 2, 3] {\n    print("n =", n);\n}',
            explanation:
                '`while` loops run as long as the condition is true. `for`…`in` iterates over arrays and ranges.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '6. Collections & Arrays',
            subtitle: 'Fixed and dynamic list data structures',
            codeSnippet: 'let arr = [10, 20, 30];\nprint("First:", arr[0]);\nprint("Length:", arr.len());\n\nlet empty: [i32] = [];',
            explanation:
                'Array literals use square brackets. Indexing starts at 0. Use `.len()` to get the length.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '7. Pattern Matching (`match`)',
            subtitle: 'Exhaustive branching on values',
            codeSnippet: 'let grade = "B";\nmatch grade {\n    "A" => print("Excellent!"),\n    "B" => print("Good"),\n    "C" => print("Average"),\n    _   => print("Other"),\n}',
            explanation:
                '`match` is like a powerful switch statement. The `_` arm is a catch-all wildcard. Matches are exhaustive — all cases must be handled.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '8. Structs',
            subtitle: 'Custom data types',
            codeSnippet: 'struct Point {\n    x: f64,\n    y: f64,\n}\n\nlet p = Point { x: 3.0, y: 4.0 };\nprint("x:", p.x, "y:", p.y);',
            explanation:
                'Structs group related data into a named type. Fields are accessed with dot notation.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '9. Ownership & Memory Safety',
            subtitle: 'Zero-GC memory model',
            codeSnippet: 'let s1 = "hello";\nlet s2 = s1;   // s1 is moved\n// print(s1);  // Error: s1 is moved\nprint(s2);     // OK',
            explanation:
                'AdeshLang uses an ownership model similar to Rust. Each value has a single owner. When ownership is transferred (moved), the original binding becomes invalid. This ensures memory safety without a garbage collector.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '10. Return Values',
            subtitle: 'Explicit and implicit returns',
            codeSnippet: 'fn square(n: i32) -> i32 {\n    return n * n;\n}\n\nfn cube(n: i32) -> i32 {\n    n * n * n  // implicit return\n}',
            explanation:
                'Use `return` to return a value explicitly. The last expression in a function body can also serve as the implicit return value (no semicolon needed).',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '11. Type Annotations',
            subtitle: 'Explicit type declarations',
            codeSnippet: 'let age: i32 = 25;\nlet pi: f64 = 3.14159;\nlet name: str = "Adesh";\nlet active: bool = true;',
            explanation:
                'Supported primitive types: `i32` (integer), `f64` (float), `str` (string), `bool` (boolean). Types are optional when they can be inferred.',
          ),
          const SizedBox(height: 12),
          _buildTopicCard(
            title: '12. Comments',
            subtitle: 'Documenting your code',
            codeSnippet:
                '// This is a single-line comment\n\n/*\n  This is a\n  multi-line comment\n*/\n\nlet x = 42; // inline comment',
            explanation:
                'Use `//` for single-line comments and `/* */` for multi-line block comments.',
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }

  Widget _buildTopicCard({
    required String title,
    required String subtitle,
    required String codeSnippet,
    required String explanation,
  }) {
    return Card(
      color: AppTheme.surface,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title,
                style: const TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.bold,
                    color: AppTheme.primary)),
            Text(subtitle,
                style: const TextStyle(
                    fontSize: 12, color: AppTheme.textSecondary)),
            const SizedBox(height: 10),
            Container(
              padding: const EdgeInsets.all(12),
              width: double.infinity,
              decoration: BoxDecoration(
                color: AppTheme.editorBackground,
                borderRadius: BorderRadius.circular(8),
                border: Border.all(color: AppTheme.border, width: 1),
              ),
              child: SelectableText(
                codeSnippet,
                style: const TextStyle(
                    fontFamily: 'monospace',
                    fontSize: 13,
                    color: AppTheme.secondary),
              ),
            ),
            const SizedBox(height: 10),
            Text(
              explanation,
              style: const TextStyle(
                  fontSize: 13, color: AppTheme.textPrimary, height: 1.4),
            ),
          ],
        ),
      ),
    );
  }
}
