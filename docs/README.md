# PiBeat Documentation

Welcome to the PiBeat documentation! This directory contains comprehensive guides for using and developing with PiBeat.

## 📚 Documentation Index

### User Guides

- **[AGENT_GUIDE.md](AGENT_GUIDE.md)** - Complete guide to using the AI agent
  - Setting up API keys (OpenAI, Anthropic, Local mode)
  - Model selection and configuration
  - Usage examples and best practices
  - Understanding agent capabilities and limitations

- **[DEBUGGING_AGENT.md](DEBUGGING_AGENT.md)** - Troubleshooting the AI agent
  - How to debug API key issues
  - Understanding console logs
  - Common problems and solutions
  - Testing with different providers

### Technical References

- **[REACTIVE_AGENT_FEATURES.md](REACTIVE_AGENT_FEATURES.md)** - Advanced agent capabilities
  - Automatic token limit handling
  - Task splitting for complex requests
  - Iterative reflection and self-improvement
  - Smart context management
  - Retry logic with exponential backoff

- **[PARSER_LIMITATIONS.md](PARSER_LIMITATIONS.md)** - Sonic Pi parser features and limitations
  - Newly supported features (define blocks, randomization, rings, spreads)
  - Supported scales and chords (50+ scales, 20+ chord types)
  - Remaining limitations (MIDI, control, sync)
  - Full feature compatibility matrix

- **[LLM_API_COMPATIBILITY.md](LLM_API_COMPATIBILITY.md)** - LLM API compatibility guide
  - OpenAI API differences (GPT-4 vs GPT-5 series)
  - Anthropic Claude API structure
  - Parameter changes and breaking updates
  - Error handling and common issues

### Historical References

- **[REACTIVE_AGENT_CHANGES.md](REACTIVE_AGENT_CHANGES.md)** - Implementation changelog (v1 → v2)
  - Historical record of reactive agent enhancement
  - API signature changes
  - Migration notes for developers
  - ⚠️ **Note**: This is a historical document. For current features, see REACTIVE_AGENT_FEATURES.md

## 🚀 Quick Start

1. **New user?** Start with [AGENT_GUIDE.md](AGENT_GUIDE.md) to set up your API keys and learn how to use the agent
2. **Having issues?** Check [DEBUGGING_AGENT.md](DEBUGGING_AGENT.md) for troubleshooting steps
3. **Want to understand parser capabilities?** See [PARSER_LIMITATIONS.md](PARSER_LIMITATIONS.md)
4. **Using OpenAI or Anthropic APIs?** Refer to [LLM_API_COMPATIBILITY.md](LLM_API_COMPATIBILITY.md)

## 📝 Contributing

When adding new features or making changes:

1. Update relevant documentation in this folder
2. Update `.github/copilot-instructions.md` for agent system context
3. Keep documentation clear, concise, and example-driven

## 🔗 Related Files

- **[../.github/copilot-instructions.md](../.github/copilot-instructions.md)** - System context for the AI agent (includes all Sonic Pi knowledge)
- **[../README.md](../README.md)** - Main project README
