import 'package:flutter/services.dart' show rootBundle;

class ExampleItem {
  final String id;
  final String title;
  final String category;
  final String assetPath;
  final String description;

  ExampleItem({
    required this.id,
    required this.title,
    required this.category,
    required this.assetPath,
    required this.description,
  });
}

class ExampleService {
  static final List<ExampleItem> examples = [
    ExampleItem(
      id: '01',
      title: 'Hello World',
      category: 'Basics',
      assetPath: 'assets/examples/01_hello_world.adesh',
      description: 'Your first program in AdeshLang',
    ),
    ExampleItem(
      id: '02',
      title: 'Variables & Types',
      category: 'Basics',
      assetPath: 'assets/examples/02_variables.adesh',
      description: 'Learn variables, explicit typing, and inference',
    ),
    ExampleItem(
      id: '03',
      title: 'Functions',
      category: 'Functions',
      assetPath: 'assets/examples/03_functions.adesh',
      description: 'Define functions, return values, and parameters',
    ),
    ExampleItem(
      id: '04',
      title: 'Conditions & Branching',
      category: 'Control Flow',
      assetPath: 'assets/examples/04_conditions.adesh',
      description: 'If, else if, and conditional statements',
    ),
    ExampleItem(
      id: '05',
      title: 'Loops & Iteration',
      category: 'Control Flow',
      assetPath: 'assets/examples/05_loops.adesh',
      description: 'While loops and pattern iteration',
    ),
    ExampleItem(
      id: '06',
      title: 'Collections',
      category: 'Data Structures',
      assetPath: 'assets/examples/06_collections.adesh',
      description: 'Arrays, indexing, and collection operations',
    ),
    ExampleItem(
      id: '07',
      title: 'Structs',
      category: 'Data Structures',
      assetPath: 'assets/examples/07_structs.adesh',
      description: 'Composite structures and fields',
    ),
    ExampleItem(
      id: '08',
      title: 'Modules & Functions',
      category: 'Modules',
      assetPath: 'assets/examples/08_modules.adesh',
      description: 'Modular program organization',
    ),
    ExampleItem(
      id: '09',
      title: 'Standard Library',
      category: 'Stdlib',
      assetPath: 'assets/examples/09_stdlib.adesh',
      description: 'Built-in I/O and utility helpers',
    ),
    ExampleItem(
      id: '10',
      title: 'Fibonacci Recursion',
      category: 'Advanced',
      assetPath: 'assets/examples/10_advanced.adesh',
      description: 'Recursive algorithms in AdeshLang',
    ),
  ];

  static Future<String> loadExampleContent(ExampleItem example) async {
    try {
      return await rootBundle.loadString(example.assetPath);
    } catch (_) {
      return '// Example content for ${example.title}\nprint("${example.title}");\n';
    }
  }
}
