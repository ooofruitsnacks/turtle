use super::{clipped, retrieved_at};
use anyhow::{bail, ensure, Context, Result};
use reqwest::{Client, Response, Url};
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::time::Instant;

const USER_AGENT: &str = "TurtleWeb/1.0";
const MAX_PAGE_BYTES: usize = 1024 * 1024;
const MAX_TEXT_BYTES: usize = 8000;

pub(super) fn normalize_url(value: &str) -> Result<Url> {
    ensure!(
        !value.is_empty()
            && value.len() <= 2048
            && !value.contains('\\')
            && !value.chars().any(|c| c.is_control() || c.is_whitespace()),
        "invalid URL"
    );

    let mut url = Url::parse(value)?;

    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "only HTTP and HTTPS URLs are permitted"
    );

    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are prohibited"
    );

    let host = url.host_str().context("URL has no hostname")?;
    let plain_host = host.trim_start_matches('[').trim_end_matches(']');

    if let Ok(ip) = plain_host.parse::<IpAddr>() {
        ensure!(public_ip(ip), "non-public destination");
    } else {
        let host = host.trim_end_matches('.').to_ascii_lowercase();

        ensure!(
            host.contains('.')
                && host != "localhost"
                && ![".localhost", ".local", ".internal", ".home.arpa"]
                    .iter()
                    .any(|suffix| host.ends_with(suffix)),
            "local or single-label hostname prohibited"
        );
    }

    let expected = if url.scheme() == "https" { 443 } else { 80 };

    ensure!(
        url.port_or_known_default() == Some(expected),
        "only the scheme's standard port is permitted"
    );

    url.set_fragment(None);
    Ok(url)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();

            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113))
        }

        IpAddr::V6(ip) => {
            let s = ip.segments();

            // Conservative global-unicast subset. Exclude transition,
            // documentation, and special-purpose blocks.
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && s[1] <= 0x01ff)
                && !(s[0] == 0x2001 && s[1] == 0x0db8)
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] <= 0x0fff)
        }
    }
}

async fn public_addresses(url: &Url) -> Result<Vec<SocketAddr>> {
    let host = url.host_str().context("missing hostname")?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = url.port_or_known_default().context("missing port")?;

    let mut addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .context("DNS lookup timed out")??
        .collect()
    };

    ensure!(
        !addresses.is_empty() && addresses.len() <= 64,
        "unexpected DNS result count"
    );

    ensure!(
        addresses.iter().all(|address| public_ip(address.ip())),
        "DNS resolved to a prohibited address"
    );

    addresses.sort_unstable();
    addresses.dedup();
    Ok(addresses)
}

async fn request(url: &Url) -> Result<Response> {
    let addresses = public_addresses(url).await?;
    let host = url.host_str().context("missing hostname")?;

    // A fresh pinned client prevents a new hostname lookup or reuse of
    // an old connection from bypassing the destination validation.
    let client = Client::builder()
        .no_proxy()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .pool_max_idle_per_host(0)
        .resolve_to_addrs(host, &addresses)
        .user_agent(USER_AGENT)
        .build()?;

    Ok(client
        .get(url.clone())
        .header("Accept-Encoding", "identity")
        .header("Accept", "text/html,text/plain,application/json;q=0.8")
        .send()
        .await?)
}

pub(super) async fn bounded_body(mut response: Response, maximum: usize) -> Result<Vec<u8>> {
    if let Some(length) = response.content_length() {
        ensure!(length <= maximum as u64, "response exceeds byte limit");
    }

    let mut bytes = Vec::new();

    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= maximum,
            "response exceeds byte limit"
        );
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}

struct RobotsPolicy {
    body: String,
    delay: Duration,
}

#[derive(Default)]
pub(super) struct PageReader {
    policies: HashMap<String, RobotsPolicy>,
    next_request: HashMap<String, Instant>,
}

impl PageReader {
    async fn allow_and_wait(&mut self, url: &Url) -> Result<()> {
        let origin = url.origin().ascii_serialization();

        if !self.policies.contains_key(&origin) {
            ensure!(self.policies.len() < 32, "robots-policy budget exhausted");

            let robots_url = normalize_url(&format!("{origin}/robots.txt"))?;
            let response = request(&robots_url).await?;
            let status = response.status().as_u16();

            let body = match status {
                404 | 410 => String::new(),

                200..=299 => {
                    let encoding = response
                        .headers()
                        .get("content-encoding")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("identity");

                    ensure!(
                        encoding.eq_ignore_ascii_case("identity"),
                        "compressed robots policy refused"
                    );

                    String::from_utf8(bounded_body(response, 128 * 1024).await?)
                        .context("robots policy is not UTF-8")?
                }

                _ => bail!(
                    "cannot establish robots policy: HTTP {status}; \
                     robots redirects are not automatically followed"
                ),
            };

            // Conservative handling of crawl extensions: honor the
            // largest supplied delay, even if it belongs to another group.
            let mut delay = 1.0_f64;

            for line in body.lines() {
                let line = line.split('#').next().unwrap_or("").trim();

                let Some((name, value)) = line.split_once(':') else {
                    continue;
                };

                if name.trim().eq_ignore_ascii_case("request-rate") {
                    bail!("robots request-rate policy is not supported");
                }

                if name.trim().eq_ignore_ascii_case("crawl-delay") {
                    let seconds: f64 =
                        value.trim().parse().context("invalid robots crawl delay")?;

                    ensure!(
                        seconds.is_finite() && (0.0..=10.0).contains(&seconds),
                        "robots crawl delay exceeds this tool's budget"
                    );

                    delay = delay.max(seconds);
                }
            }

            let delay = Duration::from_secs_f64(delay);

            self.next_request
                .insert(origin.clone(), Instant::now() + delay);
            self.policies
                .insert(origin.clone(), RobotsPolicy { body, delay });
        }

        let policy = &self.policies[&origin];
        let mut matcher = robotstxt::DefaultMatcher::default();

        ensure!(
            matcher.one_agent_allowed_by_robots(&policy.body, "TurtleWeb", url.as_str(),),
            "robots.txt disallows this URL"
        );

        let delay = policy.delay;

        if let Some(next) = self.next_request.get(&origin).copied() {
            tokio::time::sleep_until(next).await;
        }

        self.next_request.insert(origin, Instant::now() + delay);
        Ok(())
    }

    pub async fn fetch(&mut self, value: &str) -> Result<Value> {
        let mut url = normalize_url(value)?;

        for redirect in 0..=4 {
            self.allow_and_wait(&url).await?;
            let response = request(&url).await?;
            let status = response.status();

            if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
                ensure!(redirect < 4, "redirect limit exceeded");

                let location = response
                    .headers()
                    .get("location")
                    .context("redirect has no Location")?
                    .to_str()?;

                let next = normalize_url(url.join(location)?.as_str())?;

                ensure!(
                    !(url.scheme() == "https" && next.scheme() == "http"),
                    "HTTPS-to-HTTP downgrade refused"
                );

                url = next;
                continue;
            }

            ensure!(status.is_success(), "page returned HTTP {status}");

            let encoding = response
                .headers()
                .get("content-encoding")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("identity");

            ensure!(
                encoding.eq_ignore_ascii_case("identity"),
                "compressed response refused"
            );

            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();

            ensure!(
                content_type.starts_with("text/") || content_type == "application/json",
                "only HTML and text documents are supported"
            );

            let bytes = bounded_body(response, MAX_PAGE_BYTES).await?;
            let source = String::from_utf8(bytes).context("page is not UTF-8 text")?;

            let html = content_type == "text/html";
            let base = url.clone();

            let (text, links) = if html {
                tokio::task::spawn_blocking(move || extract_html(&source, &base))
                    .await
                    .context("HTML extraction worker failed")??
            } else {
                (source, Vec::new())
            };

            return Ok(json!({
                "ok": true,
                "kind": "untrusted_web_page",
                "url": url.as_str(),
                "retrieved_at_unix": retrieved_at(),
                "content_type": content_type,
                "text": clipped(&text, MAX_TEXT_BYTES),
                "truncated": text.len() > MAX_TEXT_BYTES,
                "links": links
            }));
        }

        bail!("redirect limit exceeded")
    }
}

fn extract_html(source: &str, base: &Url) -> Result<(String, Vec<String>)> {
    let document = Html::parse_document(source);
    let mut text = String::new();

    for node in document.root_element().descendants() {
        let Some(value) = node.value().as_text() else {
            continue;
        };

        let hidden = node.ancestors().any(|ancestor| {
            ancestor.value().as_element().is_some_and(|element| {
                matches!(
                    element.name(),
                    "script" | "style" | "noscript" | "svg" | "template"
                )
            })
        });

        if hidden {
            continue;
        }

        for word in value.split_whitespace() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(word);

            // Keep one extra byte/character to signal truncation.
            if text.len() > MAX_TEXT_BYTES {
                break;
            }
        }

        if text.len() > MAX_TEXT_BYTES {
            break;
        }
    }

    let selector = Selector::parse("a[href]")
        .map_err(|_| anyhow::anyhow!("invalid built-in link selector"))?;

    let mut links = Vec::new();
    let mut used = 0;

    for element in document.select(&selector) {
        let Some(href) = element.value().attr("href") else {
            continue;
        };

        let Ok(joined) = base.join(href) else {
            continue;
        };

        let Ok(url) = normalize_url(joined.as_str()) else {
            continue;
        };

        let url = url.to_string();

        if links.contains(&url) || used + url.len() > 4000 {
            continue;
        }

        used += url.len();
        links.push(url);

        if links.len() == 10 {
            break;
        }
    }

    Ok((text, links))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_addresses() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2002:7f00:1::",
            "2001:db8::1",
        ] {
            assert!(!public_ip(value.parse().unwrap()), "{value}");
        }
    }

    #[test]
    fn accepts_normal_public_addresses() {
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn rejects_unsafe_urls() {
        for value in [
            "file:///etc/passwd",
            "http://localhost/",
            "http://service.internal/",
            "http://printer/",
            "https://user:password@example.com/",
            "https://example.com:8443/",
            "http://127.0.0.1/",
            "http://[::1]/",
        ] {
            assert!(normalize_url(value).is_err(), "{value}");
        }
    }

    #[test]
    fn removes_fragment() {
        assert_eq!(
            normalize_url("https://example.com/docs#part")
                .unwrap()
                .as_str(),
            "https://example.com/docs"
        );
    }

    #[test]
    fn extracts_text_without_scripts() {
        let base = Url::parse("https://example.com/docs/").unwrap();

        let (text, links) = extract_html(
            "<script>hidden</script>\
             <style>also hidden</style>\
             <p>Visible documentation</p>\
             <a href='../reference'>Reference</a>",
            &base,
        )
        .unwrap();

        assert!(!text.contains("hidden"));
        assert!(text.contains("Visible documentation"));
        assert_eq!(links, vec!["https://example.com/reference"]);
    }
}
