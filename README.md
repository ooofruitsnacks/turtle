> [!IMPORTANT]
>  There are 2 versions of turtle, Forge Mistral and Forge Ollama. Go to the releases section for a full step by step installation, setup, and usage guide.

## Welcome to turtle 🐢

Turtle is an LLM assistant program that runs locally on your device. Other general flagship models are way faster, yes I know, but with turtle you don't have your information and data being used to train other models. You can use turtle offline with no API keys and you have full control over the agent's harness. Flagship models don't offer this, sometimes being slower is better. The big difference with turtle is that you don't need to give turtle system wide access to read,write, and compile code, this means an AI model can't delete all your files by accident like *cough* other companies *cough*

## What is turtle?

Turtle will soon be a family tree of different sized text, multimodal, speech, and embedding models. For now, the only variant of turtle is known as Forge. Forge is a text model that focuses solely on programming. Forge currently has 2 versions known as Forge Mistral and Forge Ollama. These versions both function the same but they have different backend support. Forge Mistral uses Mistralrs 0.8.1 for back end support, if you are going to be running a model with less than 16GB of ram Forge Mistral is perfect for you. Mistralrs only supports dense architecture, it does not support MoE architecture. The background threads will panic while loading MoE models and poison the engine internal lock. This issue has been solved with a different backend support and lead to the new updated version, Forge Ollama. Forge Ollama uses llama.cpp for the backend which supports MoE for Apple silicon specifically. Ollama has much better optimization for metal support as well, giving you more efficiency with token usage and model response time/accuracy. Memory management is now automatic with template handling built in per model as well. 

## Turtle Features

### Context Brain Map

With the v.0.2 release of Forge Ollama there was many newly added features focusing on performance and token usage. The first feature is what I refer to as the Context Brain Map for the AI Model. This brain map is used for the AI model to keep track of information in the conversation resulting in far more efficient token usage, GPU usage, internal temperature during runtime, and memory management/allocation for the model itself while prompting. This context brain map acts as a symbol table with episodic memory along with a unique RAG system. "RAG" is short for retrieval-augmented generation, instead of wasting resources inserting everything into the prompt, you store condensed structured data and retrieve only the relevant slice per query. This saves token usage massively while returning the same level of quality responses. There are 3 parts to the context brain map, known as the symbol map, error memory, and decision log. The symbol map is a complete index of the source code broken down into text chunks by definitions of functions, classes, and variables. It's then indexed as a reference of where those symbols are called and used in the source code which the model then identifies and links. The error memory records bugs/errors in the source code. While turtle is debugging, it will store the attempt data in the memory of the conversation whether it's successful or not, so it can pull from that information later. The error memory also stores successful completed bug fixes, this ensures the model doesn't get stuck in a loop performing fixes already applied and reduces the chances of hallucinations. This information gathered by error memory is then used for the next part of the context brain map. Using the attempted and completed debugging data, a decision log is created with a complete history of the initial plan phase, all changes/fixes applied, failed attempts, and returned errors. This decision log guides the model with the choice of the next step to choose while working. Before a piece of data is stored into one of the three parts of the context brain map, the data must be broken down into a "chunk". The RAG system implemented into the context brain map includes chunk retrieval/splitting features. This is how turtle is able to read pieces of information from the source code instead of the entire file, line by line, every time. The data is split into "chunks" and stored. This method allows the model to save on context limits while receiving all the necessary data. There are 4 main chunk categories, these are known as Token chunking, Sentence chunking, Recursive chunking (respects document structure) and Semantic chunking (groups by meaning). This chunking system is used to optimize the retriever so the model doesn't split the source mid sentence. This reduces the chances of poisoned results drastically and allows the model to receive more useful context without using more tokens to do so. This one feature alone provides massive performance improvements, accuracy from the model with the response, model prompt and model loading times, thermal throttling from your machine/hardware, and allows for higher context limits within your prompt itself. 

### RAG Pipeline

1. Ingest
2. Chunk (4 Strategies)
3. Embed
4. Vector Store (L2 Normalized Vectors)

### Chunk Strategy Selection 

| Strategy  | Speed  | Boundary safety | Best for |
| --------- | ------- | ----------------  | -------- |
Token | Fastest | ❌ Can cut mid-sentence | Uniform logs, raw diagnostic dumps
Sentence | Fast | ✅ Never mid-sentence | Prose docs, error explanations
Recursive | Fast | ✅ Respects headers/paragraphs/functions | Markdown, source code, structured docs
Semantic | Slowest (embeds every sentence) | ✅ Groups by actual meaning | High-value docs (system prompts, specs) ingested once

### Local Embedding

Local embedding model is now nomic-embed-text, a long-context encoder that outperforms OpenAI's text-embedding-ada-002 on both short and long context retrieval and runs entirely offline through Ollama backend. This is the third step in the RAG pipeline and resulted 5x faster performance on average. 


### What is the difference between Forge Mistral and Forge Ollama?

As mentioned above, turtle will eventually be a family tree of many models. Forge is the programming variant of turtle, Forge has 2 versions to meet the varying demands of users. For those who don't have a lot of RAM or newer hardware you can run the Mistral version of Forge with mistralrs backend support. Forge Ollama uses llama.cpp backend to provide even faster responses with higher accuracy but it is more demanding with hardware and resources. Both versions of Forge do the same thing, they run AI models locally on your device offline. 

### Family Tree

<img width="644" height="364" alt="Screenshot 2026-07-28 at 3 57 09 PM" src="https://github.com/user-attachments/assets/3ad2e33a-0bfb-467e-9ad5-b14009ed81d1" />

***

The version you're looking for can be downloaded/copied by going to the bottom of that release and downloading the zip from the assets or by simply changing the branch from the main branch to either the Forge Mistral or Forge Ollama branch and copying from your terminal. 

<img width="712" height="120" alt="Screenshot 2026-07-27 at 9 23 35 PM" src="https://github.com/user-attachments/assets/c018afec-5bf4-4909-9724-adfdd372f8a9" />

## 🐢 turtle : Forge ⚒️

###  🦙 - Forge Ollama

###  🇫🇷 - Forge Mistral

| Language           | Compatibility      | Forge Version Supported |
| ------------------ | ------------------ | ----------------------- |
| Rust               | :white_check_mark: |🦙✅🇫🇷✅  
| Odin               | :white_check_mark: |🦙✅🇫🇷⛔️ 
| Go                 | :no_entry:         |
| C / C++            | :no_entry:         |
| C#                 | :no_entry:         |
| Java               | :no_entry:         |
| Python             | :no_entry:         |
| Ruby               | :no_entry:         |
| Swift              | :no_entry:         |

>[!NOTE]
>ODIN COMPILE ISSUES WITH FORGE MISTRAL


