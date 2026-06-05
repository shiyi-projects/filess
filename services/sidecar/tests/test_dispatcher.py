from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from sidecar.main import handle_line


class DispatcherTestCase(unittest.TestCase):
    def test_parse_item_rpc_returns_result(self) -> None:
        response = handle_line(
            '{"jsonrpc":"2.0","id":"1","method":"build_features","params":{"parsed_item":{"name":"a.txt","extension":".txt"}}}'
        )

        self.assertEqual(response["id"], "1")
        self.assertIn("result", response)
        self.assertEqual(response["result"]["feature_text"], "name=a.txt\nextension=.txt")


if __name__ == "__main__":
    unittest.main()
