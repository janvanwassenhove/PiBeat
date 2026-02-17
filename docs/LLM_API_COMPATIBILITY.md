# LLM API Compatibility Guide

This document explains the API differences between OpenAI and Anthropic models, and how PiBeat handles them.

## OpenAI Models

### GPT-5 Series (gpt-5.2, gpt-5-mini, gpt-5-nano)

**Breaking Changes from GPT-4:**

| Parameter | GPT-4 | GPT-5 | Notes |
|-----------|-------|-------|-------|
| Token limit | `max_tokens` | `max_completion_tokens` | Different parameter name |
| Temperature | `0.0` - `2.0` | `1.0` only | Custom values not supported |
| Default temp | `1.0` | `1.0` | Same default |
| Output limit | 2000-4096 | 8192 | Higher limit needed for good responses |

**Example API Call:**
```typescript
{
  model: 'gpt-5-mini',
  messages: [...],
  max_completion_tokens: 8192, // Increased from 2000 to prevent truncation
  // No temperature parameter - only default (1.0) supported
}
```

**Important:** GPT-5 models can hit the token limit very quickly with long system contexts. 
The app uses 8192 tokens to ensure enough room for responses.

### GPT-4 Series (gpt-4o, gpt-4o-mini)

**Legacy Parameters (still working):**

```typescript
{
  model: 'gpt-4o',
  messages: [...],
  max_tokens: 4096, // Increased from 2000
  temperature: 0.7, // Customizable
}
```

## Anthropic Models

### All Claude Models (3.5 Sonnet, 3.5 Haiku, Sonnet 4.5)

**API Structure:**
- System message passed separately via `system` parameter
- Higher token limits than OpenAI
- Different message format

**Example API Call:**
```typescript
{
  model: 'claude-sonnet-4.5',
  system: 'You are a helpful assistant...',
  messages: [
    { role: 'user', content: '...' },
    { role: 'assistant', content: '...' }
  ],
  max_tokens: 4096, // Anthropic standard
  temperature: 1.0, // Default
}
```

## PiBeat Implementation

### Automatic Detection

The app automatically detects model version and applies correct parameters:

```typescript
// In callOpenAI()
const isGPT5 = config.model.startsWith('gpt-5');

if (isGPT5) {
  // GPT-5: max_completion_tokens, no custom temperature
  completionParams.max_completion_tokens = 2000;
} else {
  // GPT-4: max_tokens, custom temperature
  completionParams.max_tokens = 2000;
  completionParams.temperature = 0.7;
}
```

### Error Handling

Common API errors and their meanings:

| Error | Cause | Solution |
|-------|-------|----------|
| `400: Unsupported parameter 'max_tokens'` | Using GPT-4 params with GPT-5 | Fixed automatically in latest code |
| `400: Unsupported value: 'temperature'` | Custom temperature with GPT-5 | Fixed automatically (omits parameter) |
| `finishReason: 'length'` with no content | Token limit too low for context | Fixed: Increased to 8192 (GPT-5) / 4096 (GPT-4) |
| `401: Invalid API key` | Wrong or expired key | Check Settings (⚙️) |
| `429: Rate limit exceeded` | Too many requests | Wait or switch to Local mode |
| `404: Model not found` | Invalid model ID or no access | Select different model or verify access |

### Testing Your Changes

After updating the code, test with:

1. **GPT-5 Mini** - Fast, good for testing parameter compatibility
2. **Claude 3.5 Haiku** - Fast, good for testing Anthropic integration
3. **Local mode** - No API key needed, good for fallback testing

### Console Logs

Enable DevTools (F12) and look for:
- `[callOpenAI] Calling with params:` - Shows parameters sent to OpenAI
- `[callAnthropic] Calling with model:` - Shows Anthropic model being called
- `[LLM] Using API key from config:` - Confirms API key detected

### Common Gotchas

1. **Don't hardcode temperature for new models** - Check model version first
2. **Don't assume parameter names** - GPT-5 uses different names than GPT-4
3. **Test with actual API keys** - Errors only appear when calling real APIs
4. **Check Anthropic docs** - They have different limits and structure than OpenAI

## Future-Proofing

When OpenAI/Anthropic release new models:

1. Check their API documentation for breaking changes
2. Update model detection logic in `callOpenAI()` or `callAnthropic()`
3. Add new model IDs to `AVAILABLE_MODELS` in `src/llm.ts`
4. Test with a real API key before committing
5. Update this document with new model requirements

## References

- [OpenAI API Docs](https://platform.openai.com/docs/api-reference/chat/create)
- [Anthropic API Docs](https://docs.anthropic.com/claude/reference/messages_post)
- [PiBeat LLM Implementation](src/llm.ts)
