# Welcome to Forge Ollama!  🐢⚒️🦙
>[!IMPORTANT]
>APPLE SILICON ONLY CURRENTLY

Everything you're familiar with in the original Forge version (now known as Forge Mistral) but optimized for Ollama backend support. This release note will provide all the information you need to get turtle up and running to Forge Ollama so be sure to read through everything. Don't feel pressured to switch back over though, I'm providing support for both Forge Ollama and Forge Mistral :) 

## Why an Ollama version? What's the difference between Mistral?

Mistralrs 0.8.1 is great for smaller back end support, if you are going to be running a model for less than 16GB of ram Forge Mistral is plenty for you. Mistralrs only supports dense architecture, it does not support MoE (mix of experts) architecture. The background threads will panic while loading MoE models and poison the engine internal lock. Ollama uses llama.cpp which supports MoE for apple silicon. Ollama has much better optimization for metal support as well, giving you more efficiency with token usage and model response time/accuracy. Memory management is now automatic with template handling built in per model. 

## Is it still offline?

Yes! and No! You will need an internet connection in order to download your model locally to your device, after it's downloaded you no longer need internet access. Your model will be stored to ```~/.ollama/models/``` and every time you use turtle this is what's happening under the hood.

1. Starts a local HTTP server
2. Turtle sends requests to local host
3. Model is generated 100% with your hardware and then used for responses

***

## How to use Forge Ollama:

```
git clone -b Forge-Ollama https://github.com/ooofruitsnacks/turtle.git
```

and then run ```cd turtle``` followed by ```cargo build --release``` in your terminal

>[!NOTE]
>Create a new folder/sub-directory within turtle-ollama named models and save your model .gguf inside of that folder/sub-directory. Unlike Forge Mistral there is no need for chat instructions.

<img width="576" height="225" alt="Screenshot 2026-07-27 at 8 37 00 PM" src="https://github.com/user-attachments/assets/27675eb3-db30-4c06-b33b-15c4b2c2d516" />


<img width="325" height="200" alt="Screenshot 2026-07-27 at 8 37 07 PM" src="https://github.com/user-attachments/assets/62b36263-acb7-4781-91ea-13956da6a852" />


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

**Full Changelog**: https://github.com/ooofruitsnacks/turtle/commits/Forge_Ollama
