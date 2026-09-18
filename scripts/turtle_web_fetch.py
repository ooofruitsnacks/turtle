#!/usr/bin/env python3
"""Bounded public-web reader used by turtle_web.py.

No JavaScript, cookies, authentication, environment proxies, or downloads
to project files. The caller must enforce an overall subprocess timeout.
"""

import http.client
import ipaddress
import json
import socket
import ssl
import sys
import time

from datetime import datetime, timezone
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit, urlunsplit
from urllib.robotparser import RobotFileParser


USER_AGENT = "TurtleWeb/1.0"
MAX_PAGE_BYTES = 1_048_576
MAX_ROBOTS_BYTES = 131_072
MAX_TEXT_BYTES = 6_000
MAX_REDIRECTS = 4

REDIRECTS = {301, 302, 303, 307, 308}
BLOCKED_SUFFIXES = (".localhost", ".local", ".internal", ".home.arpa")

# Be conservative around special-purpose address ranges.
BLOCKED_V4 = tuple(
    ipaddress.ip_network(value)
    for value in (
        "192.0.0.0/24",
        "192.88.99.0/24",
        "198.18.0.0/15",
    )
)

GLOBAL_V6 = ipaddress.ip_network("2000::/3")
BLOCKED_V6 = tuple(
    ipaddress.ip_network(value)
    for value in (
        "2001::/23",
        "2002::/16",
        "3fff::/20",
    )
)


def clipped(text, limit):
    raw = text.encode("utf-8")
    return raw[:limit].decode("utf-8", errors="ignore")


def normalize_url(value):
    if not isinstance(value, str) or not value or len(value) > 2048:
        raise ValueError("URL must contain between 1 and 2048 characters")

    if "\\" in value or any(ord(c) <= 32 or ord(c) == 127 for c in value):
        raise ValueError("URL contains whitespace, controls, or backslashes")

    parts = urlsplit(value)

    if parts.scheme.lower() not in ("http", "https"):
        raise ValueError("Only HTTP and HTTPS URLs are allowed")

    if parts.username is not None or parts.password is not None:
        raise ValueError("URL credentials are prohibited")

    host = parts.hostname
    if not host:
        raise ValueError("URL has no hostname")

    host = host.rstrip(".").lower()
    if "%" in host:
        raise ValueError("Scoped or escaped hostnames are prohibited")

    host = host.encode("idna").decode("ascii")

    if (
        host == "localhost"
        or "." not in host and ":" not in host
        or host.endswith(BLOCKED_SUFFIXES)
    ):
        raise ValueError("Local or single-label hostnames are prohibited")

    default_port = 443 if parts.scheme.lower() == "https" else 80
    port = parts.port or default_port

    if port != default_port:
        raise ValueError("Only the scheme's standard public port is allowed")

    netloc = f"[{host}]" if ":" in host else host
    return urlunsplit(
        (parts.scheme.lower(), netloc, parts.path or "/", parts.query, "")
    )


def public_ip(value):
    address = ipaddress.ip_address(value)

    if not address.is_global:
        return False

    if isinstance(address, ipaddress.IPv4Address):
        return not any(address in network for network in BLOCKED_V4)

    return (
        address in GLOBAL_V6
        and not any(address in network for network in BLOCKED_V6)
    )


def resolve_public(host, port):
    records = socket.getaddrinfo(
        host,
        port,
        type=socket.SOCK_STREAM,
        proto=socket.IPPROTO_TCP,
    )
    addresses = list(dict.fromkeys(record[4][0] for record in records))

    if not addresses:
        raise ValueError("Hostname did not resolve")

    # Reject the whole result set if it mixes public/private addresses.
    if not all(public_ip(address) for address in addresses):
        raise ValueError("Destination resolves to a non-public address")

    return addresses


class PinnedConnection(http.client.HTTPConnection):
    """Connect to an already-validated IP while preserving Host and TLS SNI."""

    def __init__(self, host, port, address, use_tls):
        super().__init__(host, port, timeout=10)
        self.address = address
        self.use_tls = use_tls

    def connect(self):
        sock = socket.create_connection(
            (self.address, self.port),
            timeout=self.timeout,
        )

        try:
            if self.use_tls:
                context = ssl.create_default_context()
                sock = context.wrap_socket(sock, server_hostname=self.host)
            self.sock = sock
        except BaseException:
            sock.close()
            raise


def request_once(url, maximum):
    url = normalize_url(url)
    parts = urlsplit(url)
    port = 443 if parts.scheme == "https" else 80
    addresses = resolve_public(parts.hostname, port)

    # The connection is pinned; HTTP cannot perform a second hostname lookup.
    connection = PinnedConnection(
        parts.hostname,
        port,
        addresses[0],
        parts.scheme == "https",
    )

    target = parts.path or "/"
    if parts.query:
        target += "?" + parts.query

    try:
        connection.request(
            "GET",
            target,
            headers={
                "User-Agent": USER_AGENT,
                "Accept": "text/html,text/plain,application/json;q=0.8",
                "Accept-Encoding": "identity",
                "Connection": "close",
            },
        )

        response = connection.getresponse()
        headers = {key.lower(): value for key, value in response.getheaders()}

        if response.status in REDIRECTS:
            return response.status, headers, b""

        encoding = headers.get("content-encoding", "identity").lower()
        if encoding not in ("identity", ""):
            raise ValueError("Compressed response refused")

        length = headers.get("content-length")
        if length is not None and int(length) > maximum:
            raise ValueError("Response exceeds the download limit")

        body = response.read(maximum + 1)
        if len(body) > maximum:
            raise ValueError("Response exceeds the download limit")

        return response.status, headers, body
    finally:
        connection.close()


def origin(url):
    parts = urlsplit(url)
    return urlunsplit((parts.scheme, parts.netloc, "", "", ""))


def robots_policy(url):
    robots_url = origin(url) + "/robots.txt"
    status, _, body = request_once(robots_url, MAX_ROBOTS_BYTES)

    if status in (404, 410):
        return None, 1.0

    if status in (401, 403):
        raise ValueError("Website denies automated access through robots.txt")

    # Fail closed rather than follow uncertain robots-policy redirects.
    if not 200 <= status < 300:
        raise ValueError(
            f"Cannot establish robots policy: HTTP {status}"
        )

    policy = RobotFileParser()
    policy.parse(body.decode("utf-8", errors="replace").splitlines())

    delay = float(policy.crawl_delay(USER_AGENT) or 1)
    rate = policy.request_rate(USER_AGENT)
    if rate and rate.requests:
        delay = max(delay, rate.seconds / rate.requests)

    if delay > 10:
        raise ValueError("Website crawl delay exceeds this tool's budget")

    return policy, max(1.0, delay)


class TextExtractor(HTMLParser):
    IGNORED = {"script", "style", "noscript", "svg", "template"}

    def __init__(self, base):
        super().__init__(convert_charrefs=True)
        self.base = base
        self.hidden = 0
        self.parts = []
        self.links = []

    def handle_starttag(self, tag, attrs):
        if tag in self.IGNORED:
            self.hidden += 1

        if self.hidden:
            return

        if tag == "a":
            href = dict(attrs).get("href")
            if href and len(self.links) < 20:
                try:
                    link = normalize_url(urljoin(self.base, href))
                except (ValueError, UnicodeError):
                    return

                if link not in self.links:
                    self.links.append(link)

    def handle_endtag(self, tag):
        if tag in self.IGNORED and self.hidden:
            self.hidden -= 1

    def handle_data(self, data):
        if not self.hidden:
            self.parts.append(data)

    def text(self):
        return " ".join(" ".join(self.parts).split())


def fetch_page(value):
    url = normalize_url(value)
    policies = {}

    for _ in range(MAX_REDIRECTS + 1):
        site = origin(url)

        if site not in policies:
            policies[site] = robots_policy(url)

        policy, delay = policies[site]

        if policy is not None and not policy.can_fetch(USER_AGENT, url):
            raise ValueError("robots.txt disallows this URL")

        time.sleep(delay)
        status, headers, body = request_once(url, MAX_PAGE_BYTES)

        if status in REDIRECTS:
            location = headers.get("location")
            if not location:
                raise ValueError("Redirect has no Location header")

            next_url = normalize_url(urljoin(url, location))
            if url.startswith("https://") and next_url.startswith("http://"):
                raise ValueError("HTTPS-to-HTTP downgrade refused")

            url = next_url
            continue

        if not 200 <= status < 300:
            raise ValueError(f"Website returned HTTP {status}")

        content_type = headers.get("content-type", "").split(";")[0].lower()

        if not (
            content_type.startswith("text/")
            or content_type == "application/json"
        ):
            raise ValueError("Only HTML and text responses are supported")

        # UTF-8 is deliberately the initial supported decoding mode.
        text = body.decode("utf-8", errors="replace")
        links = []

        if content_type == "text/html":
            extractor = TextExtractor(url)
            extractor.feed(text)
            text = extractor.text()
            links = extractor.links

        truncated = len(text.encode("utf-8")) > MAX_TEXT_BYTES

        return {
            "ok": True,
            "kind": "untrusted_web_page",
            "url": url,
            "retrieved_at": datetime.now(timezone.utc).isoformat(),
            "content_type": content_type,
            "text": clipped(text, MAX_TEXT_BYTES),
            "truncated": truncated,
            "links": links,
        }

    raise ValueError("Too many redirects")


def main():
    try:
        request = json.loads(sys.stdin.buffer.read(8193))
        result = fetch_page(request["url"])
    except Exception as error:
        result = {
            "ok": False,
            "error": clipped(str(error), 500),
        }

    json.dump(result, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()

