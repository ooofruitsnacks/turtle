use super::{clipped, random_id, retrieved_at, WebOptions};
use anyhow::{bail, ensure, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

const OWNER_LABEL: &str = "org.turtle.web.owner";
const MANAGED_LABEL: &str = "org.turtle.web.managed";

const BOOT: &str = r#"
set -eu
umask 077
mkdir -p /tmp/turtle-config /tmp/turtle-data /tmp/turtle-cache
printf '%s\n' "$TURTLE_SETTINGS" > /tmp/turtle-config/settings.yml

export SEARXNG_SETTINGS_PATH=/tmp/turtle-config/settings.yml
export SEARXNG_CONFIG_PATH=/tmp/turtle-config
export SEARXNG_DATA_PATH=/tmp/turtle-data
export XDG_CACHE_HOME=/tmp/turtle-cache
export PYTHONDONTWRITEBYTECODE=1
export GRANIAN_HOST=0.0.0.0
export GRANIAN_PORT=8080
export GRANIAN_INTERFACE=wsgi
export GRANIAN_WORKERS=1

/usr/local/searxng/.venv/bin/granian searx.webapp:app &
server=$!

(
    sleep "$TURTLE_TTL"
    kill -TERM "$server" 2>/dev/null || true
    sleep 5
    kill -KILL "$server" 2>/dev/null || true
) &
watcher=$!

trap 'kill -TERM "$server" "$watcher" 2>/dev/null || true' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
wait "$server"
"#;

async fn read_pipe<R: AsyncRead + Unpin>(pipe: R) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.take(65_537).read_to_end(&mut bytes).await?;
    ensure!(bytes.len() <= 65_536, "Docker output exceeded limit");
    Ok(bytes)
}

pub(super) struct ManagedSearch {
    options: WebOptions,
    owner: String,
    context: Option<String>,
    name: Option<String>,
    base: Option<String>,
    starts: usize,
    client: Client,
}

impl ManagedSearch {
    pub fn new(options: WebOptions) -> Result<Self> {
        let image = &options.web_image;

        let valid_tag = image
            .strip_prefix("docker.io/searxng/searxng:")
            .is_some_and(|tag| {
                !tag.is_empty()
                    && tag.len() <= 128
                    && tag
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            });

        let valid_digest = image
            .strip_prefix("docker.io/searxng/searxng@sha256:")
            .is_some_and(|digest| {
                digest.len() == 64 && digest.bytes().all(|c| c.is_ascii_hexdigit())
            });

        ensure!(
            valid_tag || valid_digest,
            "only an official SearXNG image tag or SHA-256 digest is allowed"
        );

        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(35))
            .pool_max_idle_per_host(1)
            .build()?;

        Ok(Self {
            options,
            owner: random_id(),
            context: None,
            name: None,
            base: None,
            starts: 0,
            client,
        })
    }

    async fn docker(&self, arguments: &[&str], seconds: u64) -> Result<String> {
        let mut command = Command::new("docker");

        if let Some(context) = &self.context {
            command.args(["--context", context]);
        }

        command
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command.spawn().context("could not run Docker")?;
        let stdout = child.stdout.take().context("missing Docker stdout")?;
        let stderr = child.stderr.take().context("missing Docker stderr")?;

        let (stdout, stderr, status) = tokio::time::timeout(Duration::from_secs(seconds), async {
            tokio::try_join!(read_pipe(stdout), read_pipe(stderr), async {
                Ok::<_, anyhow::Error>(child.wait().await?)
            })
        })
        .await
        .context("Docker command timed out")??;

        ensure!(
            status.success(),
            "Docker command failed: {}",
            clipped(&String::from_utf8_lossy(&stderr), 1500)
        );

        Ok(String::from_utf8(stdout)?.trim().to_owned())
    }

    async fn prepare(&mut self) -> Result<()> {
        if self.context.is_some() {
            return Ok(());
        }

        ensure!(
            std::env::var_os("DOCKER_HOST").is_none(),
            "unset DOCKER_HOST and select a local Docker context"
        );

        let context = self.docker(&["context", "show"], 10).await?;
        ensure!(!context.is_empty(), "Docker context is empty");

        let encoded = self
            .docker(
                &[
                    "context",
                    "inspect",
                    &context,
                    "--format",
                    "{{json .Endpoints.docker.Host}}",
                ],
                10,
            )
            .await?;

        let endpoint: String = serde_json::from_str(&encoded)?;

        ensure!(
            endpoint.starts_with("unix://"),
            "managed web tools require a local Unix-socket Docker context"
        );

        self.context = Some(context);
        self.docker(&["info", "--format", "{{.ServerVersion}}"], 15)
            .await?;

        Ok(())
    }

    async fn inspect(&self, name: &str) -> Result<Value> {
        let text = self.docker(&["inspect", name], 10).await?;

        let values: Vec<Value> = serde_json::from_str(&text)?;
        values.into_iter().next().context("empty Docker inspection")
    }

    fn owns(&self, inspection: &Value) -> bool {
        inspection["Config"]["Labels"][MANAGED_LABEL].as_str() == Some("1")
            && inspection["Config"]["Labels"][OWNER_LABEL].as_str() == Some(self.owner.as_str())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        let Some(name) = self.name.clone() else {
            return Ok(());
        };

        // Determine existence without confusing a stopped daemon with
        // an absent container.
        let names = self
            .docker(
                &[
                    "ps",
                    "--all",
                    "--filter",
                    &format!("name=^{name}$"),
                    "--format",
                    "{{.Names}}",
                ],
                10,
            )
            .await?;

        if !names.lines().any(|line| line == name) {
            self.name = None;
            self.base = None;
            return Ok(());
        }

        let inspection = self.inspect(&name).await?;
        ensure!(
            self.owns(&inspection),
            "refusing to remove an unowned container"
        );

        if let Err(error) = self.docker(&["stop", "--time", "5", &name], 12).await {
            eprintln!("Graceful web-container stop failed: {error:#}");
        }

        // --rm may already have removed the container.
        let names = self
            .docker(
                &[
                    "ps",
                    "--all",
                    "--filter",
                    &format!("name=^{name}$"),
                    "--format",
                    "{{.Names}}",
                ],
                10,
            )
            .await?;

        if names.lines().any(|line| line == name) {
            let inspection = self.inspect(&name).await?;
            ensure!(self.owns(&inspection), "container ownership changed");

            self.docker(&["rm", "--force", "--volumes", &name], 10)
                .await?;
        }

        self.name = None;
        self.base = None;
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        self.prepare().await?;

        if let Some(name) = &self.name {
            if let Ok(inspection) = self.inspect(name).await {
                ensure!(self.owns(&inspection), "container ownership mismatch");

                if inspection["State"]["Running"].as_bool() == Some(true) && self.base.is_some() {
                    return Ok(());
                }
            }

            self.shutdown().await?;
        }

        ensure!(self.starts < 2, "search-container restart budget exhausted");
        self.starts += 1;

        if self.options.pull_web_image {
            eprintln!("Downloading approved SearXNG image...");
            self.docker(&["pull", &self.options.web_image], 300).await?;
            self.options.pull_web_image = false;
        }

        let image_id = self
            .docker(
                &[
                    "image",
                    "inspect",
                    "--format",
                    "{{.Id}}",
                    &self.options.web_image,
                ],
                15,
            )
            .await
            .context(
                "SearXNG image unavailable; use --pull-web-image \
                 to authorize downloading it",
            )?;

        ensure!(
            image_id.starts_with("sha256:")
                && image_id.len() == 71
                && image_id[7..].bytes().all(|c| c.is_ascii_hexdigit()),
            "unexpected Docker image ID"
        );

        let name = format!("turtle-web-{}", &random_id()[..24]);
        self.name = Some(name.clone());

        let settings = format!(
            "use_default_settings:\n\
             \x20 engines:\n\
             \x20   keep_only:\n\
             \x20     - duckduckgo\n\
             \x20     - bing\n\
             server:\n\
             \x20 secret_key: \"{}\"\n\
             \x20 limiter: false\n\
             \x20 image_proxy: false\n\
             search:\n\
             \x20 formats:\n\
             \x20   - html\n\
             \x20   - json\n\
             engines:\n\
             \x20 - name: duckduckgo\n\
             \x20   disabled: false\n\
             \x20 - name: bing\n\
             \x20   disabled: false\n",
            random_id()
        );

        let owner_label = format!("{OWNER_LABEL}={}", self.owner);
        let managed_label = format!("{MANAGED_LABEL}=1");
        let settings_env = format!("TURTLE_SETTINGS={settings}");
        let ttl_env = format!("TURTLE_TTL={}", self.options.web_ttl_secs);

        eprintln!("Starting task-owned SearXNG container...");

        self.docker(
            &[
                "run",
                "--detach",
                "--rm",
                "--init",
                "--name",
                &name,
                "--label",
                &owner_label,
                "--label",
                &managed_label,
                "--publish",
                "127.0.0.1::8080",
                "--user",
                "searxng",
                "--workdir",
                "/usr/local/searxng",
                "--read-only",
                "--tmpfs",
                "/tmp:rw,nosuid,noexec,size=128m,mode=1777",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges:true",
                "--memory",
                "768m",
                "--memory-swap",
                "768m",
                "--cpus",
                "1",
                "--pids-limit",
                "128",
                "--stop-timeout",
                "5",
                "--log-driver",
                "local",
                "--log-opt",
                "max-size=5m",
                "--log-opt",
                "max-file=1",
                "--env",
                &settings_env,
                "--env",
                &ttl_env,
                "--entrypoint",
                "/bin/sh",
                &image_id,
                "-c",
                BOOT,
            ],
            30,
        )
        .await?;

        let inspection = self.inspect(&name).await?;
        ensure!(self.owns(&inspection), "new container ownership mismatch");

        let binding = &inspection["NetworkSettings"]["Ports"]["8080/tcp"][0];

        ensure!(
            binding["HostIp"].as_str() == Some("127.0.0.1"),
            "unexpected published host address"
        );

        let port: u16 = binding["HostPort"]
            .as_str()
            .context("Docker did not assign a host port")?
            .parse()?;

        let base = format!("http://127.0.0.1:{port}");

        for _ in 0..30 {
            let ready = self
                .client
                .get(format!("{base}/"))
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success());

            if ready {
                self.base = Some(base);
                eprintln!("SearXNG is ready.");
                return Ok(());
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        bail!("SearXNG failed its readiness deadline")
    }

    pub async fn search(&mut self, query: &str) -> Result<Value> {
        self.start().await?;
        let base = self
            .base
            .as_ref()
            .context("search service has no endpoint")?;

        let response = self
            .client
            .get(format!("{base}/search"))
            .query(&[
                ("q", query),
                ("format", "json"),
                ("engines", "duckduckgo,bing"),
            ])
            .send()
            .await?
            .error_for_status()?;

        let bytes = super::fetch::bounded_body(response, 2 * 1024 * 1024).await?;
        let payload: Value = serde_json::from_slice(&bytes)?;

        let entries = payload["results"]
            .as_array()
            .context("search service returned no results array")?;

        let mut results = Vec::new();

        for entry in entries {
            let Some(raw_url) = entry["url"].as_str() else {
                continue;
            };

            let Ok(url) = super::fetch::normalize_url(raw_url) else {
                continue;
            };

            results.push(json!({
                "title": clipped(entry["title"].as_str().unwrap_or(""), 250),
                "url": url.as_str(),
                "snippet": clipped(
                    entry["content"].as_str().unwrap_or(""),
                    500
                )
            }));

            if results.len() == 5 {
                break;
            }
        }

        Ok(json!({
            "ok": true,
            "kind": "untrusted_search_snippets",
            "retrieved_at_unix": retrieved_at(),
            "query": query,
            "results": results,
            "notice": "Search snippets only; linked pages have not been read."
        }))
    }
}
