class AppSettings {
  double fontSize;
  int tabSize;
  String fontFamily;
  bool showLineNumbers;
  bool wordWrap;
  bool enableSyntaxHighlighting;
  bool enableAutosave;
  bool enableAutoFormat;
  bool autoCloseBrackets;
  bool enableAutocomplete;
  bool confirmDelete;
  bool autoOpenConsole;

  AppSettings({
    this.fontSize = 14.0,
    this.tabSize = 4,
    this.fontFamily = 'JetBrains Mono',
    this.showLineNumbers = true,
    this.wordWrap = false,
    this.enableSyntaxHighlighting = true,
    this.enableAutosave = true,
    this.enableAutoFormat = false,
    this.autoCloseBrackets = true,
    this.enableAutocomplete = true,
    this.confirmDelete = true,
    this.autoOpenConsole = true,
  });

  Map<String, dynamic> toJson() {
    return {
      'fontSize': fontSize,
      'tabSize': tabSize,
      'fontFamily': fontFamily,
      'showLineNumbers': showLineNumbers,
      'wordWrap': wordWrap,
      'enableSyntaxHighlighting': enableSyntaxHighlighting,
      'enableAutosave': enableAutosave,
      'enableAutoFormat': enableAutoFormat,
      'autoCloseBrackets': autoCloseBrackets,
      'enableAutocomplete': enableAutocomplete,
      'confirmDelete': confirmDelete,
      'autoOpenConsole': autoOpenConsole,
    };
  }

  factory AppSettings.fromJson(Map<String, dynamic> json) {
    return AppSettings(
      fontSize: (json['fontSize'] as num?)?.toDouble() ?? 14.0,
      tabSize: (json['tabSize'] as num?)?.toInt() ?? 4,
      fontFamily: json['fontFamily'] as String? ?? 'JetBrains Mono',
      showLineNumbers: json['showLineNumbers'] as bool? ?? true,
      wordWrap: json['wordWrap'] as bool? ?? false,
      enableSyntaxHighlighting: json['enableSyntaxHighlighting'] as bool? ?? true,
      enableAutosave: json['enableAutosave'] as bool? ?? true,
      enableAutoFormat: json['enableAutoFormat'] as bool? ?? false,
      autoCloseBrackets: json['autoCloseBrackets'] as bool? ?? true,
      enableAutocomplete: json['enableAutocomplete'] as bool? ?? true,
      confirmDelete: json['confirmDelete'] as bool? ?? true,
      autoOpenConsole: json['autoOpenConsole'] as bool? ?? true,
    );
  }
}
