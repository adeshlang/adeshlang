"""Unit and Integration Tests for AdeshLang MCP Server."""

import asyncio
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "mcp"))

import adesh_mcp.server as mcp_server


class TestAdeshLangMCP(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = mcp_server.create_server()

    def test_server_creation(self):
        self.assertIsNotNone(self.server)
        self.assertEqual(self.server.name, "AdeshLang MCP")

    def test_registered_tools(self):
        tools = asyncio.run(self.server.list_tools())
        tool_names = [t.name for t in tools]

        # Check key adesh tools
        self.assertIn("adesh_run", tool_names)
        self.assertIn("adesh_build", tool_names)
        self.assertIn("adesh_check", tool_names)
        self.assertIn("adesh_format", tool_names)
        self.assertIn("adesh_doctor", tool_names)
        self.assertIn("adesh_gpu_check", tool_names)

        # Check key adl tools
        self.assertIn("adl_new", tool_names)
        self.assertIn("adl_init", tool_names)
        self.assertIn("adl_build", tool_names)
        self.assertIn("adl_run", tool_names)
        self.assertIn("adl_test", tool_names)
        self.assertIn("adl_doctor", tool_names)
        self.assertIn("adl_graph", tool_names)

        # Check file helpers
        self.assertIn("create_file", tool_names)
        self.assertIn("read_file", tool_names)

        self.assertGreaterEqual(len(tools), 50)

    def test_registered_resources(self):
        resources = asyncio.run(self.server.list_resources())
        uris = [str(r.uri) for r in resources]

        self.assertIn("adesh://docs/language-reference", uris)
        self.assertIn("adesh://docs/cli-reference", uris)
        self.assertIn("adesh://docs/stdlib", uris)
        self.assertIn("adesh://templates/manifest", uris)

    def test_registered_prompts(self):
        prompts = asyncio.run(self.server.list_prompts())
        prompt_names = [p.name for p in prompts]

        self.assertIn("create_adesh_project", prompt_names)
        self.assertIn("debug_adesh_code", prompt_names)
        self.assertIn("optimize_performance", prompt_names)

    def test_tool_execution_doctor(self):
        with patch(
            "adesh_mcp.tools.run_cli_command",
            return_value={"success": True, "exit_code": 0, "stdout": "", "stderr": ""},
        ):
            result = asyncio.run(self.server.call_tool("adesh_doctor", {}))
        self.assertIsNotNone(result)

    def test_tool_execution_file_operations(self):
        base_dir = os.getcwd()
        with tempfile.TemporaryDirectory(dir=base_dir) as tmpdir:
            test_path = os.path.relpath(
                os.path.join(tmpdir, "mcp_test_file.adesh"), base_dir
            )
            test_content = 'fn main() { print("Hello MCP"); }'

            create_res = asyncio.run(
                self.server.call_tool(
                    "create_file", {"filepath": test_path, "content": test_content}
                )
            )
            self.assertIsNotNone(create_res)

            read_res = asyncio.run(
                self.server.call_tool("read_file", {"filepath": test_path})
            )
            self.assertIsNotNone(read_res)


if __name__ == "__main__":
    unittest.main()
