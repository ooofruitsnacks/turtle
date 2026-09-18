#!/usr/bin/env python3
"""Offline tests for Turtle's managed web integration."""

import json
import socket
import unittest

from unittest.mock import patch

from turtle_web import (
    MANAGED_LABEL,
    OWNER_LABEL,
    MAX_EVIDENCE_BYTES,
    WebSession,
    extended_schema,
    owned,
)
from turtle_web_fetch import (
    TextExtractor,
    normalize_url,
    public_ip,
    resolve_public,
)


class URLTests(unittest.TestCase):
    def test_normalizes_public_url(self):
        self.assertEqual(
            normalize_url("https://Example.COM/docs#section"),
            "https://example.com/docs",
        )

    def test_rejects_unsafe_url_shapes(self):
        examples = [
            "file:///etc/passwd",
            "http://localhost/",
            "http://service.internal/",
            "http://printer/",
            "http://user:password@example.com/",
            "https://example.com:8443/",
            "https://example.com/\nheader",
            "https://example.com\\@localhost/",
        ]

        for example in examples:
            with self.subTest(url=example):
                with self.assertRaises((ValueError, UnicodeError)):
                    normalize_url(example)

    def test_rejects_private_and_special_addresses(self):
        addresses = [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2002:7f00:1::",
        ]

        for address in addresses:
            with self.subTest(address=address):
                self.assertFalse(public_ip(address))

    def test_accepts_normal_public_addresses(self):
        self.assertTrue(public_ip("8.8.8.8"))
        self.assertTrue(public_ip("2606:4700:4700::1111"))

    def test_rejects_mixed_public_private_dns(self):
        records = [
            (
                socket.AF_INET,
                socket.SOCK_STREAM,
                socket.IPPROTO_TCP,
                "",
                ("8.8.8.8", 443),
            ),
            (
                socket.AF_INET,
                socket.SOCK_STREAM,
                socket.IPPROTO_TCP,
                "",
                ("127.0.0.1", 443),
            ),
        ]

        with patch("socket.getaddrinfo", return_value=records):
            with self.assertRaises(ValueError):
                resolve_public("example.com", 443)

    def test_returns_validated_addresses(self):
        records = [
            (
                socket.AF_INET,
                socket.SOCK_STREAM,
                socket.IPPROTO_TCP,
                "",
                ("8.8.8.8", 443),
            ),
        ]

        with patch("socket.getaddrinfo", return_value=records):
            self.assertEqual(
                resolve_public("example.com", 443),
                ["8.8.8.8"],
            )


class ExtractionTests(unittest.TestCase):
    def test_omits_script_and_style_text(self):
        parser = TextExtractor("https://example.com/docs/")
        parser.feed(
            "<style>hidden CSS</style>"
            "<script>hidden JS</script>"
            "<p>Visible documentation</p>"
            '<a href="../reference">Reference</a>'
        )

        self.assertNotIn("hidden", parser.text())
        self.assertIn("Visible documentation", parser.text())
        self.assertEqual(
            parser.links,
            ["https://example.com/reference"],
        )

    def test_does_not_register_non_http_links(self):
        parser = TextExtractor("https://example.com/")
        parser.feed('<a href="javascript:alert(1)">bad</a>')
        self.assertEqual(parser.links, [])


class SchemaTests(unittest.TestCase):
    def test_extends_without_mutating_original(self):
        original = {
            "anyOf": [
                {
                    "type": "object",
                    "properties": {
                        "action": {"enum": ["edit"]},
                    },
                },
                {
                    "type": "object",
                    "properties": {
                        "action": {"enum": ["stop"]},
                    },
                },
            ]
        }

        result = extended_schema(original)

        self.assertEqual(len(original["anyOf"]), 2)
        self.assertEqual(len(result["anyOf"]), 4)
        self.assertEqual(
            result["anyOf"][2]["required"],
            ["action", "query"],
        )
        self.assertFalse(result["anyOf"][3]["additionalProperties"])

    def test_rejects_missing_action_schema(self):
        with self.assertRaises(RuntimeError):
            extended_schema("json")


class OwnershipTests(unittest.TestCase):
    def test_only_matching_task_is_owned(self):
        labels = {
            MANAGED_LABEL: "1",
            OWNER_LABEL: "task-one",
        }
        self.assertTrue(owned(labels, "task-one"))
        self.assertFalse(owned(labels, "task-two"))
        self.assertFalse(owned({}, "task-one"))
        self.assertFalse(owned(None, "task-one"))


class EvidenceTests(unittest.TestCase):
    def test_evidence_is_bounded(self):
        session = WebSession.__new__(WebSession)
        session.evidence = []
        session.omitted = 0

        for index in range(30):
            session.remember({
                "index": index,
                "text": "x" * 4000,
            })

        encoded = json.dumps(session.evidence).encode("utf-8")
        self.assertLessEqual(len(encoded), MAX_EVIDENCE_BYTES)
        self.assertGreater(session.omitted, 0)
        self.assertEqual(session.evidence[-1]["index"], 29)


if __name__ == "__main__":
    unittest.main()

