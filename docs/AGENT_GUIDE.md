# Agent Chat - LLM Integration Guide

## 🚀 New: Fully Reactive Agent

The agent now features **advanced reactive capabilities**:
- ✅ **Automatic token limit handling** — Continues truncated responses seamlessly
- ✅ **Task splitting** — Intelligently breaks complex requests into subtasks
- ✅ **Iterative self-improvement** — Up to 3 reflection cycles for quality
- ✅ **Smart context management** — Prevents context overflow in long chats
- ✅ **Retry logic** — Handles transient errors automatically

**📖 See [REACTIVE_AGENT_FEATURES.md](REACTIVE_AGENT_FEATURES.md) for detailed documentation of all reactive features.**

---

## Overview

The PiBeat agent now supports **three modes**:

1. **Local (Offline)** — Pattern-matching with built-in Sonic Pi knowledge (no API key needed)
2. **OpenAI** — GPT-5.2, GPT-5 Mini, GPT-5 Nano, GPT-4o (legacy), GPT-4o Mini (legacy)
3. **Anthropic** — Claude Sonnet 4.5, Claude 3.5 Sonnet (legacy), Claude 3.5 Haiku

## Features

### Reactive Agent Architecture
- **Multi-turn reasoning** — The agent can reflect on its own responses and improve them
- **Up to 3 reflection cycles** — Ensures higher quality answers with better iteration
- **Automatic continuation** — Handles token limit truncation transparently (up to 3 continuations)
- **Task splitting** — Detects and splits complex multi-part requests automatically
- **Context-aware** — Always has access to your current buffer code
- **Conversation history** — Remembers the full chat context (last 10 messages)
- **Smart retry** — Exponential backoff on transient errors
- **Quality checks** — Detects unclosed code blocks, incomplete responses, missing elements

### Model Selection
Click the 🤖 robot icon in the toolbar to open the agent chat. At the top you'll see:
- **Provider dropdown** — Choose Local, OpenAI, or Anthropic
- **Model dropdown** — Select the specific model (changes based on provider)

### API Key Setup

**Option 1: System Environment Variables (Recommended for Permanent Setup)**
1. **Windows**: 
   - Open System Properties → Advanced → Environment Variables
   - Add `OPENAI_API_KEY` and/or `ANTHROPIC_API_KEY` as User or System variables
   - Restart the app
2. **PowerShell (temporary, current session only)**:
   ```powershell
   $env:OPENAI_API_KEY="sk-..."
   $env:ANTHROPIC_API_KEY="sk-ant-..."
   ```
   Then run the app from the same PowerShell window

**Option 2: .env File (Recommended for Development)**
1. Copy `.env.example` to `.env` in the project root
2. Fill in your API keys:
   ```bash
   OPENAI_API_KEY=sk-...
   ANTHROPIC_API_KEY=sk-ant-...
   ```
3. Restart the app

**Option 3: Settings UI (Convenient for Quick Testing)**
1. Click the ⚙️ settings (gear) icon in the agent panel header
2. Enter your API keys:
   - **OpenAI**: Get from [platform.openai.com/api-keys](https://platform.openai.com/api-keys)
   - **Anthropic**: Get from [console.anthropic.com/settings/keys](https://console.anthropic.com/settings/keys)
3. Click "Save Keys"

**Priority**: The agent checks in this order:
1. System environment variables (via Tauri API) ← Most secure, persists across restarts
2. .env file variables (build-time Vite env) ← Good for development
3. localStorage (Settings UI) ← Quick and convenient

## Usage Examples

### Generate Code
```
"Create a techno beat with kick, hihat, and snare"
"Make an acid bassline using the tb303 synth"
"Build a full ambient track with pads and arpeggios"
```

### Refactor Your Code
```
"Refactor my code to be cleaner"
"Improve this code structure"
"Break this into separate live loops"
```

### Explain & Analyze
```
"Explain what this code does"
"Check my code for issues"
"What's wrong with this loop?"
```

### Code Insertion
When the agent generates code:
- Click **Insert** to append it to your current buffer
- Click **Replace** to overwrite your entire buffer with the new code

## System Context

The agent has complete knowledge of:
- All Sonic Pi syntax (synths, samples, effects, loops, rings, threads)
- The PiBeat application (buffers, keyboard shortcuts, sample browser, effects panel)
- Music coding best practices (always use `sleep`, prefer `live_loop`, etc.)

This knowledge is injected from [.github/copilot-instructions.md](../.github/copilot-instructions.md) as system context in every LLM call.

## Local vs. LLM Mode

| Feature | Local | OpenAI/Anthropic |
|---------|-------|------------------|
| Works offline | ✅ Yes | ❌ No |
| Requires API key | ❌ No | ✅ Yes |
| Response quality | Good (rule-based) | Excellent (LLM reasoning) |
| Cost | Free | Pay per token |
| Speed | Fast | Varies by model |
| Reflection | ❌ No | ✅ Yes |
| Understands context | Partial | Deep |

**Recommendation**: Use **Local** for quick pattern generation and **Claude Sonnet 4.5** or **GPT-5.2** for complex requests and refactorings.

## Troubleshooting

### "Sorry, I encountered an error"
- Check your API key in settings
- Verify you have credits/quota on OpenAI/Anthropic
- Switch to Local mode as a fallback

### Slow responses
- Try a faster model (GPT-5 Nano, GPT-5 Mini, Claude 3.5 Haiku)
- Check your internet connection

### Agent not available
- Make sure the agent panel is open (click 🤖 in toolbar)
- Refresh the app if the panel doesn't appear

## Privacy & Security

- API keys stored in `localStorage` (never sent to our servers — we don't have any!)
- Keys are only sent directly to OpenAI/Anthropic APIs
- All code stays on your machine
- The agent runs client-side in your browser

## Updating the Knowledge Base

When you add new features to PiBeat, update [.github/copilot-instructions.md](../.github/copilot-instructions.md) to keep the agent aware. The system context is loaded from that file on every LLM call.
