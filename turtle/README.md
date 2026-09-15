# Welcome to Forge Ollama!  🐢⚒️🦙
>[!IMPORTANT]
>APPLE SILICON ONLY CURRENTLY

## Why an Ollama version? What's the difference between Mistral?

Mistralrs 0.8.1 is great for smaller back end support, if you are going to be running a model with less than 16GB of ram Forge Mistral is plenty for you. Mistralrs only supports dense architecture, it does not support MoE (mix of experts) architecture. The background threads will panic while loading MoE models and poison the engine internal lock. Ollama uses llama.cpp which supports MoE for apple silicon. Ollama has much better optimization for metal support as well, giving you more efficiency with token usage and model response time/accuracy. Memory management is now automatic with template handling built in per model. 

## Is it still offline?

Yes! and No! You will need an internet connection initially to download your model locally to your device, after it's downloaded you no longer need internet access. Your model will be stored to ```~/.ollama/models/```.

Every time you use turtle this is the process of what's happening under the hood.

1. Starts a local HTTP server
2. Turtle sends requests to local host
3. Model is generated 100% with your hardware and used for your prompt

***

## How to use Forge Ollama:

```
git clone -b Forge-Ollama https://github.com/ooofruitsnacks/turtle.git
```

and then run ```cd turtle``` followed by ```cargo build --release``` in your terminal


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
