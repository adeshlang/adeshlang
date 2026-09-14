# Path Standard Library Documentation for AdeshLang

The `Path` standard library in AdeshLang provides a production-grade, type-safe, memory-safe, reference-safe, and deterministic cross-platform abstraction for working with filesystem paths across Windows, Unix, macOS, and Linux environments.

---

## 1. Overview

In AdeshLang, a filesystem path is represented as a parsed object structure (`Path`) rather than a plain string. The `Path` library automatically manages operating system path separators (`\` on Windows, `/` on POSIX), UNC network paths, Windows drive letters, root directories, relative/absolute normalization, and safe path traversal.

```adesh
import Path;
import FS;

let file = Path.new("assets")
    .join("images")
    .join("logo.png");

print(file); // Output (Windows): assets\images\logo.png
             // Output (Unix):    assets/images/logo.png
```

---

## 2. Public Classes & Types

The module exports the following primary types:

| Class | Description |
| :--- | :--- |
| `Path` | Core representation of a filesystem path with manipulation methods |
| `PathComponent` | Parsed element of a path (`Root`, `Prefix`, `Parent`, `Current`, `Normal`) |
| `PathIterator` | Iterable sequence of path components |
| `PathError` | Structured exception/error type for invalid path operations |
| `Root` | Filesystem root descriptor (e.g. `/` or `C:\`) |
| `Prefix` | Windows drive letter or UNC prefix descriptor |
| `PathBuilder` | Fluent builder pattern for incremental path construction |
| `RelativePath` | Type-safe wrapper for relative paths |
| `AbsolutePath` | Type-safe wrapper for absolute paths |

---

## 3. Construction & Instantiation

### Static Factory Methods

```adesh
// Create from string or existing path
let p1 = Path.new("images/logo.png");
let p2 = Path.fromString("docs/spec.pdf");

// Empty path
let empty = Path.empty();

// System special directories
let cwd = Path.current();      // Current working directory
let home = Path.home();        // User home directory
let temp = Path.temp();        // System temporary directory
let exe = Path.executable();   // Location of running binary
```

---

## 4. Path Manipulation & Joining

### Joining Paths

Joining components automatically converts separators to match the host operating system and normalizes redundant segments.

```adesh
let joined = Path.new("assets").join("icons").join("home.svg");
```

### Push & Pop

```adesh
let p = Path.new("usr").push("local").push("bin");
p.pop(); // Removes 'bin', leaving 'usr/local'
```

---

## 5. Path Components & Inspection

### File Name, Stem & Extensions

```adesh
let file = Path.new("assets/report.final.pdf");

print(file.fileName());       // "report.final.pdf"
print(file.stem());           // "report.final"
print(file.extension());      // "pdf"

// Modifying extensions
let web = file.withExtension("html");    // "assets/report.final.html"
let stripped = file.removeExtension();   // "assets/report.final"

// Modifying file names
let renamed = file.withFileName("index.txt"); // "assets/index.txt"
```

### Parent & Root

```adesh
let p = Path.new("/var/log/system.log");

let parent = p.parent(); // "/var/log"
let root = p.root();     // "/" (Unix) or "C:\" (Windows)
```

---

## 6. Normalization & Canonicalization

### Logical Normalization

Removes `.` (current directory), collapses `..` (parent directory) without crossing above filesystem root, and removes redundant slashes.

```adesh
let messy = Path.new("a/b/../c/./d/..");
let norm = messy.normalize(); // "a/c"
```

### Physical Canonicalization

Uses the physical OS filesystem to resolve symlinks and real casing.

```adesh
let real_path = Path.new("./symlink_to_file").canonicalize();
```

---

## 7. Absolute vs Relative

```adesh
let rel = Path.new("docs/readme.md");

print(rel.isRelative()); // true
print(rel.isAbsolute()); // false

let abs = rel.absolute(); // Converts to absolute path using CWD

let base = Path.new("/home/user");
let target = Path.new("/home/user/projects/app");
let relative = target.relativeTo(base); // "projects/app"
```

---

## 8. Filesystem Integration

The `Path` class integrates natively with AdeshLang's `FS` standard module:

```adesh
import Path;
import FS;

let path = Path.current().join("config").join("app.json");

if (path.exists()) {
    print("Is file: " + string(path.isFile()));
    print("Is directory: " + string(path.isDirectory()));

    let content = FS.readText(path);
    print(content);
}
```

---

## 9. Cross-Platform & Security Notes

- **Windows Support**: Seamlessly handles drive letters (`C:`), UNC paths (`\\server\share`), long paths, and mixed slashes (`/` and `\`).
- **Unix/Linux/macOS Support**: Handles `/`, `~`, `.`, `..`.
- **UTF-8 Unicode Awareness**: Full Unicode support for non-ASCII paths (e.g. `C:\Users\अजय`, `/home/日本/docs`, `/home/ಕನ್ನಡ`).
- **Path Traversal Security**: Normalization guarantees that `..` references cannot silently escape above root.
