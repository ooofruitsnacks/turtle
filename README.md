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

__How does turtle work?__ :turtle:

1. Starts a local HTTP server
2. Turtle sends requests to local host
3. Model is generated 100% with your hardware and used for your prompt

Turtle works by using llama.cpp backend support to pull models from Ollama locally and then making calls to that model pulled from Ollama.

***

### DEPENDENCIES 

- Homebrew
- Git
- Rust
- Ollama

__DEPENDENCIES INSTALL__

On your machine of choice, open a terminal and download Homebrew and wait for it to finish. This command can be ran on macOS, Linux or windows.

If you run into any issues please use Homebrew's guide to follow their directions. (https://brew.sh)

```
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```
After it has finished downloading onto your machine run this command to upgrade outdated packages, update Homebrew, and remove any unneeded disk space from your machine:

```
brew upgrade
brew update
brew cleanup
```
now run:

```
brew install git
```

After git has been downloaded, install rust with this command in your terminal:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

or by visiting their website, (https://rust-lang.org/tools/install/)

Now simply run:

```
brew install ollama
```

## How to install turtle:

```
git clone https://github.com/ooofruitsnacks/turtle.git
```

and then if not already in the turtle directoy run ```cd turtle``` 

To build a release of turtle, run this command:

```
cargo fmt --all && \
cargo check --all-targets && \
cargo test --all-targets && \
cargo build --release
```

Now you can use turtle, to do so, start an ```Ollama``` server and pull the ```qwen3-coder:30b``` model.

### Start an ollama server

Keep this terminal tab open as long as you want the model running. Open new terminal tabs with CMD+N and leave them running in the background. Keep in mind your model will consume ram even while at idle so close out of the model to do other work. 

```
ollama serve
```

Confirm the server is operational by checking for ``` listening on 127.0.0.1:11434 ```

Download the embedding model before running/pulling model

```
ollama pull nomic-embed-text
```

### In a new terminal tab, pull/download your AI Model. 

I recommend qwen3-coder30b on apple silicon with 32GB of unified memory/ram, currently it has performed well. 

```
ollama pull qwen3-coder:30b
```

***

## Using The New Commands

These are some examples if you get confused of how to instruct the model to use a certain language

### Python

__Executes and debugs code:__

```
./target/release/turtle \
  --model qwen3-coder:30b \
  --context 65536 \
  --language python \
  --project ./python-project // < CHANGE "./python-project" to whatever you want the output directory name to be
  --checks ./python-checks.json
  --allow-checks //
```

__Without executing project code:__

```
./target/release/turtle \
  --model qwen3-coder:30b \
  --language python \
  --project ./python-project \
  --task "Create a small Python command-line calculator with focused tests."
```
### C / C ++

```
./target/release/turtle \
  --model qwen3-coder:30b \
  --language c,cpp \
  --project ./native-project \
  --context 65536 \
  --checks ./cpp-checks.json \
  --allow-checks
```

__TypeScript with Bun, HTML, and Markdown__

```
./target/release/turtle \
  --model qwen3-coder:30b \
  --language typescript,html,markdown \
  --runtime bun \
  --project ./web-project \
  --task "Implement the requested web application changes."
```

### __Jai__

```
./target/release/turtle \
  --model qwen3-coder:30b \
  --language jai \
  --project ./jai-project
```

### Zig

Zig checks 

Requires a build.zig in the project root or for a single file project with no build.zig, use ["build-exe", "main.zig", "-femit-bin=zig-out/bin/app"] and ["test", "main.zig"] instead.
