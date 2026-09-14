import os
import subprocess
import unittest
from pathlib import Path

class IndiaLangTest(unittest.TestCase):
    BIN = Path("target/release/indialang.exe")

    EXAMPLES = [
        ("examples/basic.ind",      "a: 10 b: 5 c: 18\nHello IndiaLang!\n"),
        ("examples/if_else.ind",    "12 is even\n"),
        ("examples/loops.ind",      "Sum = 15\nLoop iteration: 1\nLoop iteration: 2\nLoop iteration: 3\n"),
        ("examples/factorial.ind",  "Factorial of 5 = 120\n"),
        ("examples/map.ind",        "Doubled: [2, 4, 6]\n"),
        ("examples/data.ind",       "Name: Ajay\nAge: 25\nFirst skill: Rust\n"),
        ("examples/class.ind",      "Hello, my name is Ajay\n"),
        ("examples/error.ind",      "Caught error: Something went wrong!\n"),
        ("examples/import_test.ind","Sum = 30\nProduct = 30\n"),
        ("examples/builtins.ind",   "Length: 4\n"),  # clock output ignored
        ("examples/nested.ind",     "Result: 20\n"),
        ("examples/custom_error.ind","Error: Division by zero!\n"),
    ]

    def run_example(self, path):
        """Run .ind file and capture stdout."""
        cmd = [str(self.BIN), "run", path]
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=5
            )
            out = proc.stdout.strip() + "\n"
            return out
        except subprocess.TimeoutExpired:
            self.fail(f"Timeout running {path}")

    def test_all_examples(self):
        """Iterate all examples and check output."""
        for path, expected in self.EXAMPLES:
            with self.subTest(example=path):
                output = self.run_example(path)
                if "clock" in expected:
                    # skip dynamic time
                    self.assertIn("Length: 4", output)
                else:
                    self.assertEqual(output, expected)

if __name__ == "__main__":
    unittest.main(verbosity=2)
