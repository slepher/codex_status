#!/usr/bin/env python3
import asyncio
import importlib.util
import pathlib
import unittest


BRIDGE_PATH = pathlib.Path(__file__).with_name("bridge.py")
spec = importlib.util.spec_from_file_location("test_bridge_transport", BRIDGE_PATH)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class FakeClient:
    def __init__(self, mtu_size=None):
        if mtu_size is not None:
            self.mtu_size = mtu_size
        self.writes = []

    async def write_gatt_char(self, uuid, data, response=True):
        self.writes.append((uuid, bytes(data), response))


class TransportTests(unittest.TestCase):
    def test_fragments_preserve_utf8_bytes_at_boundary(self):
        data = b"x" * 180 + "界尾".encode("utf-8")
        parts = bridge.fragment_payload(data)
        self.assertEqual(b"".join(parts), data)
        self.assertTrue(all(0 < len(part) <= 180 for part in parts))

    def test_fragment_bounds_and_invalid_limit(self):
        self.assertEqual(bridge.fragment_payload(b""), [])
        with self.assertRaises(ValueError):
            bridge.fragment_payload(b"x", 0)

    def test_negotiated_mtu_uses_smaller_payload(self):
        self.assertEqual(bridge.negotiated_write_limit(FakeClient(23)), 20)
        self.assertEqual(bridge.negotiated_write_limit(FakeClient(255)), 180)
        self.assertEqual(bridge.negotiated_write_limit(FakeClient()), 180)

    def test_fragmented_write_is_sequential_and_reassembles(self):
        client = FakeClient(23)
        data = "endpoint-界".encode("utf-8") * 8
        asyncio.run(bridge.write_fragmented(client, "endpoint", data))
        self.assertEqual(b"".join(chunk for _, chunk, _ in client.writes), data)
        self.assertTrue(all(len(chunk) <= 20 for _, chunk, _ in client.writes))
        self.assertTrue(all(response for _, _, response in client.writes))


if __name__ == "__main__":
    unittest.main()
