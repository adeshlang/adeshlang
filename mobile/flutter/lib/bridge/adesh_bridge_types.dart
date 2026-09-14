enum ExecutionStatus {
  idle,
  running,
  completed,
  failed,
  cancelled,
}

enum DiagnosticSeverity {
  error,
  warning,
  info,
}

class Diagnostic {
  final DiagnosticSeverity severity;
  final String message;
  final int line;
  final int column;
  final int length;
  final String? code;

  Diagnostic({
    required this.severity,
    required this.message,
    required this.line,
    required this.column,
    this.length = 1,
    this.code,
  });

  factory Diagnostic.fromJson(Map<String, dynamic> json) {
    DiagnosticSeverity severity;
    switch (json['severity']) {
      case 'warning':
        severity = DiagnosticSeverity.warning;
        break;
      case 'info':
        severity = DiagnosticSeverity.info;
        break;
      case 'error':
      default:
        severity = DiagnosticSeverity.error;
        break;
    }

    return Diagnostic(
      severity: severity,
      message: json['message'] as String? ?? 'Unknown error',
      line: (json['line'] as num?)?.toInt() ?? 1,
      column: (json['column'] as num?)?.toInt() ?? 1,
      length: (json['length'] as num?)?.toInt() ?? 1,
      code: json['code'] as String?,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'severity': severity.name,
      'message': message,
      'line': line,
      'column': column,
      'length': length,
      'code': code,
    };
  }
}

class ExecutionResult {
  final ExecutionStatus status;
  final String stdout;
  final String stderr;
  final double executionTimeMs;
  final List<Diagnostic> diagnostics;
  final String backendUsed;

  ExecutionResult({
    required this.status,
    required this.stdout,
    required this.stderr,
    required this.executionTimeMs,
    required this.diagnostics,
    this.backendUsed = 'Interpreter',
  });

  factory ExecutionResult.fromJson(Map<String, dynamic> json) {
    ExecutionStatus status;
    switch (json['status']) {
      case 'completed':
        status = ExecutionStatus.completed;
        break;
      case 'cancelled':
        status = ExecutionStatus.cancelled;
        break;
      case 'running':
        status = ExecutionStatus.running;
        break;
      case 'failed':
      default:
        status = ExecutionStatus.failed;
        break;
    }

    final diagList = (json['diagnostics'] as List<dynamic>?)
            ?.map((d) => Diagnostic.fromJson(d as Map<String, dynamic>))
            .toList() ??
        [];

    return ExecutionResult(
      status: status,
      stdout: json['stdout'] as String? ?? '',
      stderr: json['stderr'] as String? ?? '',
      executionTimeMs: (json['execution_time_ms'] as num?)?.toDouble() ?? 0.0,
      diagnostics: diagList,
      backendUsed: json['backend_used'] as String? ?? 'Interpreter',
    );
  }

  factory ExecutionResult.failure(String error, [List<Diagnostic>? diags]) {
    return ExecutionResult(
      status: ExecutionStatus.failed,
      stdout: '',
      stderr: error,
      executionTimeMs: 0.0,
      diagnostics: diags ?? [Diagnostic(severity: DiagnosticSeverity.error, message: error, line: 1, column: 1)],
      backendUsed: 'Interpreter',
    );
  }

  factory ExecutionResult.cancelled() {
    return ExecutionResult(
      status: ExecutionStatus.cancelled,
      stdout: '',
      stderr: 'Execution cancelled by user',
      executionTimeMs: 0.0,
      diagnostics: [Diagnostic(severity: DiagnosticSeverity.info, message: 'Cancelled', line: 1, column: 1)],
      backendUsed: 'Interpreter',
    );
  }
}
