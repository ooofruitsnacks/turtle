## Welcome to turtle 🐢

Turtle is an AI assistant program focused on specific tasks that runs locally on your device. Other general flagship models are way faster, yes I know, but with turtle you don't have your information and data being used to train other models. You can use turtle offline with no API keys and you have full control over the agent's harness. Flagship models don't offer this, sometimes being slower is better. The big difference with turtle is that you don't need to give turtle system wide access to read,write, and compile code, this means an AI model can't delete all your files by accident like *cough* other companies *cough*

> [!WARNING]
> 32 GB OF RAM IS RECOMMENDED TO RUN. LESS THAN 32 GB WILL SEE PERFORMANCE ISSUES.

<img width="1400" height="350" alt="Screenshot 2026-07-22 at 6 42 37 PM" src="https://github.com/user-attachments/assets/74edac52-fc71-48b8-b9c6-c96bb795c949" />

### What is turtle?

Turtle will eventually be a family tree of different sized models that specialize in varying tasks to meet the demands of user's storage/ram available space. For now, the only version of turtle is known as Forge. Forge focuses solely on coding tasks written in Rust and Odin with plans to implement other langauages soon (a full implementation guide can be found below). 

Turtle Forge is used with the model called "Qwen2.5-Coder-32B-Instruct-Q4_K_M", a 32 billion parameter model with a context length of 128k that needs 32 GB of ram and 20 GB of space to run/store the model. You can run the model with 16 GB of ram but please remember it will take much longer to run and could possibly overwork your system. This model is outdated and not as fast as other flagship models but still returns a very good result.

> [!IMPORTANT]
> Focuses on improvements, speed, and accuracy are already underway :)


🐢⚒️

| Turtle Version | Language           | Compatibility      |
| -------------  | ------------------ | ------------------ |
| Forge          | Rust               | :white_check_mark: |
|                | Odin               | :white_check_mark: |
|                | Go                 | :no_entry:         |
|                | C / C++            | :no_entry:         |
|                | C#                 | :no_entry:         |
|                | Java               | :no_entry:         |
|                | Python             | :no_entry:         |
|                | Ruby               | :no_entry:         |
|                | Swift              | :no_entry:         |

***

## EXAMPLE RUN:

Here is an example of the turtle in use. In this example I tell it to create a calculator written in rust. Turtle returned back the code in a copy/paste ready format however it did make 1 mistake that I had to correct. In "main.rs" there was an error on line 2. I had to change ```use anyhow::Result;``` to ```use anyhow::{Result, anyhow};```. A simple and quick one line fix, the response wasn't perfect but for a local model it performed very well. 

### Starting turtle

<img width="1539" height="276" alt="Screenshot 2026-07-22 at 7 42 04 PM" src="https://github.com/user-attachments/assets/34e088bc-2c74-46d7-bd1f-5f46d677b59c" />

### turtle implementing a plan to "create a simple calculator"

<img width="1513" height="311" alt="Screenshot 2026-07-22 at 7 42 26 PM" src="https://github.com/user-attachments/assets/ecf0f113-f3b8-4d91-86cb-f074ce061a6f" />

### turtle building, testing and debugging

<img width="1309" height="458" alt="Screenshot 2026-07-22 at 7 43 48 PM" src="https://github.com/user-attachments/assets/8ee244ff-382c-4695-9e67-74ea49feaea6" />

> [!IMPORTANT]
> MISSED 1 BUG!
> changed ```use anyhow::Result;``` to ```use anyhow::{Result, anyhow};```



## turtle finished results

### Cargo.toml

 <img width="506" height="220" alt="Screenshot 2026-07-22 at 7 42 44 PM" src="https://github.com/user-attachments/assets/7c29f4a6-0762-427b-bbb5-fee5800bf87e" />

 ### src/main.rs

 <img width="542" height="561" alt="Screenshot 2026-07-22 at 7 43 23 PM" src="https://github.com/user-attachments/assets/0aed20b7-c23b-4049-ba77-81c11c37588b" />

<img width="486" height="443" alt="Screenshot 2026-07-22 at 7 43 35 PM" src="https://github.com/user-attachments/assets/54d0f118-aac4-4d7c-a75b-bcc7c26cc387" />

### Usage / Results

<img width="629" height="134" alt="Screenshot 2026-07-22 at 7 41 13 PM" src="https://github.com/user-attachments/assets/6c2481e7-08cf-4a2d-a7df-7affc28eb88d" />

 



## Installation & Setup

In order to download and use turtle you must have Homebrew, Rust, Odin, Xcode(MacOS).

### Setup

Make directory with:

```
mkdir turtle
 ``` 

And change directory with:

```
cd turtle
```

### Install turtle:

``` 
git clone https://github.com/ooofruitsnacks/turtle.git
 ```

### Install The AI Model Onto Your Device

https://we.tl/t-UqgJd6FanVp2RNFc

You can use my link I made with the model saved or run the command below

```
hf download bartowski/Qwen2.5-Coder-32B-Instruct-GGUF --include "Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf" --local-dir ./models
```

After it downloads, drag and drop the file into the models sub-directory of turtle or use ```mv```

<img width="400" height="220" alt="Screenshot 2026-07-22 at 6 35 33 PM" src="https://github.com/user-attachments/assets/66755faf-db2a-4027-bd56-22f1e5b7c4f7" />


### Enable GPU Acceleration

The current "Cargo.toml" has the GPU feature enabled with 

```[dependencies]
mistralrs = { version = "0.8", features = ["metal"] }
```
Run this command in your terminal to enable this feature so there are no errors.

```
xcodebuild -downloadComponent MetalToolchain
```

> [!IMPORTANT]
> APPLE SILICON ONLY. LINUX/WINDOWS BELOW.

### NVIDIA Linux/Windows GPU Acceleration: 

Go to the "Cargo.toml" file within turtle and find this line of code near the top of the file.

```
[dependencies]
mistralrs = { version = "0.8", features = ["metal"] }
``` 

and change to 

```
[dependencies]
mistralrs = { version = "0.8", features = ["cuda"] }
```

***

## How to use

Change to turtle directory and build source. Wait a few minutes:

``` cd turtle && cargo build --release```

Run the source string. 

EXAMPLE STRING:
```
cargo run --release -- \                                                                                                                                             ─╯
  --model ./models/Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf \ 
  --chat-template ./models/qwen_chat_template.json \
  --language rust \
  --project ./out
```

If you wish to use a different programming language, change the "--language" flag in the string. 

For example this string below is using rust, if you want to use Odin simply change to ```--language odin \```

### How to breakdown turtle strings

```cargo run --release -- \``` tells turtle to run the release version you built earlier

```--model ./models/Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf \``` tells turtle what model to use

```--chat-template ./models/qwen_chat_template.json \``` tells turtle what model chat template/harness to use

```--language rust \``` tells turtle what language to write the program in

```--project ./out ``` tells turtle where to target

