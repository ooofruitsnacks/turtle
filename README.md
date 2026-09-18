<div align="center">
  
# Welcome to Turtle! :turtle:

[![License: GPL v2](https://img.shields.io/badge/License-GPL%20v2-purple.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-darkgreen)](#-download)
![GitHub Repo stars](https://img.shields.io/github/stars/oooFruitSnacks/turtle)

![GitHub Downloads (all assets, all releases)](https://img.shields.io/github/downloads/oooFruitSnacks/turtle/total)
![GitHub code size in bytes](https://img.shields.io/github/languages/code-size/oooFruitSnacks/turtle)
![GitHub commit activity](https://img.shields.io/github/commit-activity/w/oooFruitSnacks/turtle)

__SUPPORTED LANGUAGES__

![Badge](https://img.shields.io/badge/%20Rust%20-olive) ![Badge](https://img.shields.io/badge/%20C%20-olive) ![Badge](https://img.shields.io/badge/%20C++%20-olive) ![Badge](https://img.shields.io/badge/%20Jai%20-olive) ![Badge](https://img.shields.io/badge/%20Odin%20-olive) ![Badge](https://img.shields.io/badge/%20Python%20-olive) ![Badge](https://img.shields.io/badge/%20Javascript%20%7C%20Typescript%20-olive) ![Badge](https://img.shields.io/badge/%20Go%20-olive) ![Badge](https://img.shields.io/badge/%20Ruby%20-olive) ![Badge](https://img.shields.io/badge/%20Zig%20-olive) ![Badge](https://img.shields.io/badge/%20Swift%20-olive) ![Badge](https://img.shields.io/badge/%20HTML%20%7C%20Markdown%20-olive)

__A BRIEF MESSAGE FROM ME__

</div>

__Hello Users,__

I personally am not the biggest fan of LLM's and I think if they are consuming resources at the rate they do, it's an issue that needs to be solved. Sadly these companies will never cut back on that so we need to collectively make that decision to not use their products. We must self host our own models with our machines if we need or want the help of LLM's. We can't become so dependent on these services.

This is why I created turtle. To give those who want to explore, create, or learn with LLM's in a __ethical__ way. Turtle is actually a lot more powerful than you would think, try it out!!

***

__How does turtle work?__ :turtle:

1. Starts a local HTTP server
2. Turtle sends requests to local host
3. Model is generated 100% with your hardware and used for your prompt

Turtle works by using llama.cpp backend support to pull models from Ollama locally and then making calls to that model pulled from Ollama.

### DEPENDENCIES 

- Homebrew
- Git
- Rust
- Ollama

__DEPENDENCIES INSTALL__

On your machine of choice, open a terminal and download Homebrew and wait for it to finish. This command can be ran on macOS, Linux or windows.

If you run into any issues please use Homebrew's guide to follow their directions. (https://brew.sh)

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```
After it has finished downloading onto your machine run this command to upgrade outdated packages, update Homebrew, and remove any unneeded disk space from your machine:

```bash
brew upgrade
brew update
brew cleanup
```
now run:

```bash
brew install git
```

After git has been downloaded, install rust with this command in your terminal:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

or by visiting their website, (https://rust-lang.org/tools/install/)

Now simply run:

```bash
brew install ollama
```

## How to install turtle:

```bash
git clone https://github.com/ooofruitsnacks/turtle.git
```

and then if not already in the turtle directoy run ```cd turtle``` 

To build a release of turtle, run this command:

```bash
cargo fmt --all && \
cargo check --all-targets && \
cargo test --all-targets && \
cargo build --release
```

Now you can use turtle, to do so, start an ```Ollama``` server and pull the ```qwen3-coder:30b``` model.

### Start an ollama server

Keep this terminal tab open as long as you want the model running. Open new terminal tabs with CMD+N and leave them running in the background. Keep in mind your model will consume ram even while at idle so close out of the model to do other work. 

```bash
ollama serve
```

Confirm the server is operational by checking for ``` listening on 127.0.0.1:11434 ```

Download the embedding model before running/pulling model

```bash
ollama pull nomic-embed-text
```

### In a new terminal tab, pull/download your AI Model. 

I recommend qwen3-coder30b on apple silicon with 32GB of unified memory/ram, currently it has performed well. 

```bash
ollama pull qwen3-coder:30b
```

***

## Using The New Commands

These are some examples if you get confused of how to instruct the model to use a certain language

### Python

__Executes and debugs code:__

```bash
./target/release/turtle \
  --model qwen3-coder:30b \
  --context 65536 \
  --language python \
  --project ./python-project // < CHANGE "./python-project" to whatever you want the output directory name to be
  --checks ./python-checks.json
  --allow-checks //
```

__Without executing project code:__

```bash
./target/release/turtle \
  --model qwen3-coder:30b \
  --language python \
  --project ./python-project \
  --task "Create a small Python command-line calculator with focused tests."
```
### C / C ++

```bash
./target/release/turtle \
  --model qwen3-coder:30b \
  --language c,cpp \
  --project ./native-project \
  --context 65536 \
  --checks ./cpp-checks.json \
  --allow-checks
```

__TypeScript with Bun, HTML, and Markdown__

```bash
./target/release/turtle \
  --model qwen3-coder:30b \
  --language typescript,html,markdown \
  --runtime bun \
  --project ./web-project \
  --task "Implement the requested web application changes."
```

### __Jai__

```bash
./target/release/turtle \
  --model qwen3-coder:30b \
  --language jai \
  --project ./jai-project
```

### Zig

Zig checks 

Requires a build.zig in the project root or for a single file project with no build.zig, use ["build-exe", "main.zig", "-femit-bin=zig-out/bin/app"] and ["test", "main.zig"] instead.

# Turtle Environment Variables Over-rides

Turtle suports environment variables that can be altered, this can be done to make quick changes on the fly without going into ```main.rs``` or ```ollama.rs``` to make your changes. Below is a complete overview of how to use them.

## Supported overrides

| Variable | Default | Accepted values / limits | Purpose |
|---|---|---|---|
| `OLLAMA_HOST` | `http://127.0.0.1:11434` | Ollama server address | Selects the server Turtle connects to |
| `TURTLE_NUM_CTX` | `8192` | `4096`–`131072` | Total context window in tokens |
| `TURTLE_OUTPUT_TOKENS` | `4096` | `512`–`65536`; further capped at half the context window | Maximum generated tokens per agent response |
| `TURTLE_SOURCE_BYTES` | `6000` | `1000`–`64000` | Byte budget for automatically selected project-source contents |
| `TURTLE_HISTORY_TURNS` | `1` | `0`–`8` | Previous conversation turns eligible for retention |
| `TURTLE_KEEP_ALIVE` | `5m` | Ollama keep-alive setting, such as `30s`, `5m`, or `0` | Controls model residency between responses |
| `TURTLE_REQUEST_TIMEOUT_SECS` | `600` | `10`–`3600` | HTTP request timeout in seconds |
| `TURTLE_STREAM_PREVIEW` | `true` | `1`, `true`, `yes`, `0`, `false`, `no` | Enables or disables streamed terminal preview |
| `TURTLE_THINK` | Unset | `true`, `false`, `low`, `medium`, `high`, `max` | Passes a thinking setting to Ollama |

>[!NOTE]
>Turtle accepting a value does not guarantee that
Ollama or the model supports it.

## Complete default configuration

This restores the configured Turtle settings to defaults:

```bash
export OLLAMA_HOST="http://127.0.0.1:11434"
export TURTLE_NUM_CTX=8192
export TURTLE_OUTPUT_TOKENS=4096
export TURTLE_SOURCE_BYTES=6000
export TURTLE_HISTORY_TURNS=1
export TURTLE_KEEP_ALIVE="5m"
export TURTLE_REQUEST_TIMEOUT_SECS=600
export TURTLE_STREAM_PREVIEW=true

unset TURTLE_THINK
```

## Context and output budgets

Use:

```bash
export TURTLE_NUM_CTX=32768
export TURTLE_OUTPUT_TOKENS=4096
```

The total context includes:

- System instructions.
- The current task.
- Attached reference text.
- Automatically selected project source.
- Retained conversation history.
- Reserved generated output.

The backend caps requested output at half the configured context.

For example:

```bash
export TURTLE_NUM_CTX=8192
export TURTLE_OUTPUT_TOKENS=8192
```

does not request 8,192 output tokens: the backend reduces the request
to at most 4,096.

The CLI option overrides the environment variable:

```bash
export TURTLE_NUM_CTX=32768

# This invocation uses 16384, not 32768.
./target/release/turtle \
  --model YOUR_INSTALLED_MODEL \
  --project ./generated \
  --context 16384
```

The published CLI validates context against 4096–131072.
The backend also clamps its context setting.

## Project source versus attachments

This controls automatically selected project-source contents:

```bash
export TURTLE_SOURCE_BYTES=16000
```

It does not control files explicitly supplied through `--context-file`.

Explicit attachment size is a CLI setting:

```bash
--context-bytes 131072
```

The published attachment implementation has:

- Default combined raw attachment limit: 65,536 bytes.
- Maximum configurable combined limit: 1,048,576 bytes.
- Maximum attachment count: 8.

## Conversation history

```bash
export TURTLE_HISTORY_TURNS=1
```

To omit previous turns:

```bash
export TURTLE_HISTORY_TURNS=0
```

This controls history eligible for inclusion. It does not unload the
model and should not be treated as a general memory-clearing command.

## Model residency

Keep the model available briefly between responses:

```bash
export TURTLE_KEEP_ALIVE="30s"
```

Keep it available longer:

```bash
export TURTLE_KEEP_ALIVE="90s"
```

Request unloading after each response:

```bash
export TURTLE_KEEP_ALIVE="0"
```

Per-response unloading can slow multi-response tasks because the model
may reload between edits or repair attempts.

When task-level unloading is enabled:

```bash
--unload-on-exit \
--idle-unload-secs 300 \
--unload-timeout-secs 15
```

The configured idle timeout overrides `TURTLE_KEEP_ALIVE`.


>[!NOTE]
>Task end unloading remains separate from OS cache and swap management. These options do not delete downloaded models or force system swap usage to zero.

## Request timeout

Default:

```bash
export TURTLE_REQUEST_TIMEOUT_SECS=600
```

Longer allowance:

```bash
export TURTLE_REQUEST_TIMEOUT_SECS=1800
```

Maximum accepted by the current helper:

```bash
export TURTLE_REQUEST_TIMEOUT_SECS=3600
```

>[!NOTE]
>A longer timeout permits more waiting; it does not make inference faster.

## Streaming preview

Enable:

```bash
export TURTLE_STREAM_PREVIEW=true
```

Disable:

```bash
export TURTLE_STREAM_PREVIEW=false
```

>[!NOTE]
>Accepted values are case-sensitive:

| Enabled | Disabled |
|---|---|
| `1` | `0` |
| `true` | `false` |
| `yes` | `no` |


## Thinking configuration

The safest model independent default is:

```bash
unset TURTLE_THINK
```

For a model supporting boolean thinking control:

```bash
export TURTLE_THINK=false
```

or:

```bash
export TURTLE_THINK=true
```

For a model supporting thinking levels, select one:

```bash
export TURTLE_THINK=low
```

```bash
export TURTLE_THINK=medium
```

```bash
export TURTLE_THINK=high
```

```bash
export TURTLE_THINK=max
```

>[!NOTE]
>These strings are accepted by Turtle's parser. Not every model or Ollama version supports every value. An unsupported string causes Turtle to return an error. Use `unset TURTLE_THINK`, not an empty string, to omit the setting.

## Numeric parsing

Numeric values must parse as unsigned integers.

For example:

```bash
export TURTLE_NUM_CTX=32768
```

Do not use:

```bash
export TURTLE_NUM_CTX="32k"
export TURTLE_NUM_CTX="32,768"
```

## Settings without environment overrides

Use the CLI for these settings:

| Setting | CLI option |
|---|---|
| Model | `--model MODEL_NAME` |
| Project directory | `--project PATH` |
| Languages | `--language rust,python` |
| Runtime | `--runtime auto` |
| Task | `--task "Task text"` |
| Edit iteration limit | `--iterations 3` |
| Explicit reference file | `--context-file PATH` |
| Combined attachment byte limit | `--context-bytes 65536` |
| Trusted checks file | `--checks PATH` |
| Authorize checks | `--allow-checks` |
| Task-end unloading | `--unload-on-exit` |
| Idle fallback timeout | `--idle-unload-secs 300` |
| Cleanup deadline | `--unload-timeout-secs 15` |
| Configuration diagnostics | `--debug` |

Do not assume names such as these work:

```text
TURTLE_INPUT_TOKENS
TURTLE_CONTEXT_BYTES
TURTLE_MODEL
TURTLE_PROJECT
TURTLE_UNLOAD_ON_EXIT
```

The inspected implementation does not provide those overrides.

## Reset the documented settings

```bash
unset OLLAMA_HOST
unset TURTLE_NUM_CTX
unset TURTLE_OUTPUT_TOKENS
unset TURTLE_SOURCE_BYTES
unset TURTLE_HISTORY_TURNS
unset TURTLE_KEEP_ALIVE
unset TURTLE_REQUEST_TIMEOUT_SECS
unset TURTLE_STREAM_PREVIEW
unset TURTLE_THINK
unset TURTLE_INPUT_TOKENS
```

# Turtle Environment Profiles

These profiles use Turtle's documented environment variables.

Choose one profile and paste it into the terminal where you launched turtle, for example: start an ollama serve, pull your model, open turtle and paste. Then you can run turtle normally.

An explicit `--context` argument overrides `TURTLE_NUM_CTX`.

## Profile 1: Smaller context

Start here when memory use is a priority and the task is small.

```bash
export OLLAMA_HOST="http://127.0.0.1:11434"
export TURTLE_NUM_CTX=8192
export TURTLE_OUTPUT_TOKENS=2048
export TURTLE_SOURCE_BYTES=6000
export TURTLE_HISTORY_TURNS=1
export TURTLE_KEEP_ALIVE="30s"
export TURTLE_REQUEST_TIMEOUT_SECS=600
export TURTLE_STREAM_PREVIEW=true

unset TURTLE_THINK
unset TURTLE_INPUT_TOKENS
```

## Profile 2: General coding

A starting point for tasks and projects needing more source or output capacity.

```bash
export OLLAMA_HOST="http://127.0.0.1:11434"
export TURTLE_NUM_CTX=32768
export TURTLE_OUTPUT_TOKENS=4096
export TURTLE_SOURCE_BYTES=16000
export TURTLE_HISTORY_TURNS=1
export TURTLE_KEEP_ALIVE="5m"
export TURTLE_REQUEST_TIMEOUT_SECS=1200
export TURTLE_STREAM_PREVIEW=true

unset TURTLE_THINK
unset TURTLE_INPUT_TOKENS
```

## Profile 3: Large context

>[!WARNING]
>Use only if you have the available space!

```bash
export OLLAMA_HOST="http://127.0.0.1:11434"
export TURTLE_NUM_CTX=131072
export TURTLE_OUTPUT_TOKENS=8192
export TURTLE_SOURCE_BYTES=64000
export TURTLE_HISTORY_TURNS=1
export TURTLE_KEEP_ALIVE="5m"
export TURTLE_REQUEST_TIMEOUT_SECS=1800
export TURTLE_STREAM_PREVIEW=true

unset TURTLE_THINK
unset TURTLE_INPUT_TOKENS
```

>[!NOTE]
>Higher values require corresponding application changes and suitable model/runtime support. Increasing context can increase memory use and processing time. Task unloading helps between tasks, not during peak task memory use.

## Launch with task-end unloading

After selecting a profile:

```bash
./target/release/turtle \
  --model YOUR_INSTALLED_MODEL \
  --language rust \
  --project ./generated \
  --unload-on-exit \
  --idle-unload-secs 300 \
  --unload-timeout-secs 15
```

This command intentionally omits `--context`, allowing the exported
`TURTLE_NUM_CTX` value to apply.

With `--unload-on-exit`, the 300-second idle fallback overrides the
exported `TURTLE_KEEP_ALIVE`.

Add your trusted checks and authorization options as appropriate:

```bash
--checks "/absolute/path/rust-checks.json" \
--allow-checks
```

The checks file must exist. Do not add a nonexistent example path.

## Launch with a reference attachment

After selecting a profile:

```bash
./target/release/turtle \
  --model YOUR_INSTALLED_MODEL \
  --language rust \
  --project "$HOME/my-project" \
  --context-file "$HOME/Documents/reference.md" \
  --context-bytes 65536 \
  --unload-on-exit \
  --idle-unload-secs 300 \
  --unload-timeout-secs 15
```

The attachment limit is measured in raw bytes, not tokens.
The attachment must also fit the total request context.

Use a directory for `--project` and an existing UTF-8 text file for
`--context-file`.

## Verify exported values

```bash
env | sort | grep -E '^(TURTLE_|OLLAMA_HOST=)'
```

To print the configuration summary when launching Turtle, add:

```bash
--debug
```

The debug summary is not a complete dump of every environment setting.

## Troubleshooting

| Symptom | Check |
|---|---|
| Changing context has no effect | Remove or update an explicit `--context` argument |
| Context is rejected | Check the compiled application limit and model support |
| Attachment byte-limit error | Increase `--context-bytes`, not `TURTLE_SOURCE_BYTES` |
| Request exceeds context budget | Reduce attachments/source/history, reduce reserved output, or use a supported larger context |
| Generation reaches its output limit | Split the edit or increase `TURTLE_OUTPUT_TOKENS` within the available budget |
| Model reloads between responses | Check for `TURTLE_KEEP_ALIVE=0` or a short idle timeout |
| Keep-alive export seems ignored | Check whether task unloading overrides it |
| Invalid thinking-setting error | Run `unset TURTLE_THINK` |
| Model rejects a thinking level | Use a model supported value or unset the variable |
| Swap grows during a large request | Reduce context/model memory demand rather than force clearing swap |
| Exports appear in `env` but do nothing | Confirm the variable name is implemented by Turtle |



