# Welcome to Turtle!  🐢⚒️

>[!IMPORTANT]
>APPLE SILICON ONLY CURRENTLY

## Is it still offline?

Yes! and No! You will need an internet connection initially to download your model locally to your device, after it's downloaded you no longer need internet access. Your model will be stored to ```~/.ollama/models/```.

Every time you use turtle this is the process of what's happening under the hood.

1. Starts a local HTTP server
2. Turtle sends requests to local host
3. Model is generated 100% with your hardware and used for your prompt

***

### DEPENDENCIES 

- Homebrew
- Git
- Rust

## How to use turtle:

```
git clone https://github.com/ooofruitsnacks/turtle.git
```

and then if not already in the turtle directoy, 

run ```cd turtle``` 

followed by ```cargo build --release``` in your terminal.


### Install Ollama with Brew

```
brew install ollama
```

### Start an ollama server

Keep this terminal tab open as long as you want the model running. Open new terminal tabs with CMD+N and leave running in the background. 

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
## How to use turtle with ollama 

>[!NOTE]
>There are some new strings and flags added in Forge Ollama compared to Forge Mistral. You no longer need the chat flag " --chat-template" or  "--model" flag in your string. You use the Ollama model flag directly. See below for examples and usage details. 

### Build release

```
cd /turtle
cargo build --release
```

Wait for release to build. 

### Run the release

```
cargo run --release -- --model qwen3-coder:30b --language rust --project ./out
```

turtle still operates the same as Forge Mistral, give the model your idea and wait for a response to copy/paste.

### Example download and run

https://youtu.be/GWjyLu_NDO4

**Full Changelog**: https://github.com/ooofruitsnacks/turtle/commits/Forge_Ollama
