# Debugging the LLM Agent

## How to Debug API Key Issues

The agent now has detailed console logging to help debug why it might be falling back to local mode.

### Steps to Debug:

1. **Open the app** (run `npm run tauri dev`)

2. **Open DevTools Console**:
   - Press `F12` or `Ctrl+Shift+I` in the Tauri window
   - Go to the "Console" tab

3. **Select a Provider and Model**:
   - Click the 🤖 robot icon to open agent chat
   - Change provider from "Local" to "OpenAI" or "Anthropic"
   - Select a model (e.g., "GPT-5.2" or "Claude Sonnet 4.5")

4. **Send a Test Message**:
   - Type something like "generate a beat"
   - Click Send

5. **Check the Console Logs**:
   
   You should see detailed logs like:
   ```
   [AgentChat] Sending message: { provider: 'openai', model: 'gpt-5.2', hasApiKey: true, apiKeyLength: 51 }
   [LLM] Using API key from config: sk-proj-...
   [LLM] Calling openai with model gpt-5.2
   [reactiveAgentProcess] Making initial LLM call...
   [callLLM] Built messages: { provider: 'openai', messageCount: 3, roles: ['system', 'user', 'user'], ... }
   [callOpenAI] Calling with params: { model: 'gpt-5.2', hasMaxTokens: false, hasMaxCompletionTokens: true, temperature: undefined }
   [callOpenAI] Response received: { id: 'chatcmpl-...', model: 'gpt-5.2', choices: 1, hasContent: true, finishReason: 'stop' }
   [reactiveAgentProcess] Initial response length: 542
   [reactiveAgentProcess] Reflection cycle 1/2
   [reactiveAgentProcess] Reflection result: { needsMoreInfo: false, action: 'done', thought: '...' }
   [reactiveAgentProcess] Reflection complete, response is good
   [reactiveAgentProcess] Final response length: 542
   ```

   **If you see empty responses:**
   ```
   [callOpenAI] Response received: { id: '...', model: 'gpt-5-mini', choices: 1, hasContent: false, finishReason: 'stop' }
   [callOpenAI] No content in response. Full response: {...}
   [AgentChat] Agent error: Error: OpenAI returned empty response
   ```
   
   This means OpenAI API is responding but with no content - check your API key validity.

   **If you see API errors:**
   ```
   [callOpenAI] Error: BadRequestError: 400 ...
   [AgentChat] Agent error: ...
   ```
   
   Check the error message for details (invalid params, model not found, etc.)

### How to Add API Keys:

#### Option 1: Settings UI (Quickest)
1. Click the ⚙️ gear icon in the agent panel
2. Paste your API key (starts with `sk-...` for OpenAI or `sk-ant-...` for Anthropic)
3. Click "Save Keys"
4. Refresh the page or restart the app
5. Try sending a message again - check the console logs

#### Option 2: System Environment Variable
**Windows PowerShell (before launching app):**
```powershell
$env:OPENAI_API_KEY="sk-your-key-here"
$env:ANTHROPIC_API_KEY="sk-ant-your-key-here"
npm run tauri dev
```

**Windows System Properties:**
1. Right-click "This PC" → Properties → Advanced system settings
2. Click "Environment Variables"
3. Add new User variable:
   - Name: `OPENAI_API_KEY`
   - Value: `sk-your-key-here`
4. Restart the app

#### Option 3: .env File
1. Copy `.env.example` to `.env`
2. Edit `.env`:
   ```
   OPENAI_API_KEY=sk-your-key-here
   ANTHROPIC_API_KEY=sk-ant-your-key-here
   ```
3. Restart the app (Vite reads .env at startup)

### Verifying API Keys Work:

After adding a key, the console should show:
```
[getApiKey] Checking for OPENAI_API_KEY...
[getApiKey] Found in system env: sk-proj-... (or localStorage/Vite .env)
[LLM] API key from environment/storage: Found (sk-proj-...)
[LLM] Calling openai with model gpt-5.2
```

If you see "Calling openai" or "Calling anthropic", the API key is working!

### Understanding the Logs:

| Log Prefix | What It Means |
|------------|---------------|
| `[AgentChat]` | Frontend agent component |
| `[LLM]` | Main LLM coordination logic |
| `[getApiKey]` | API key retrieval from env/storage |
| `[reactiveAgentProcess]` | Reactive agent with reflection |
| `[callLLM]` | Building messages before API call |
| `[callOpenAI]` | OpenAI API call details |
| `[callAnthropic]` | Anthropic API call details |

**Key Metrics to Check:**
- `hasApiKey: true` - API key is present
- `messageCount: 3+` - Messages built correctly
- `hasContent: true` - API returned content
- `response length: 500+` - Got a real response (not empty)
- `finishReason: 'stop'` - Completed normally

### Common Issues:

1. **Empty String in localStorage**:
   - Solution: Open DevTools → Application → Local Storage → Clear `openai_api_key` and `anthropic_api_key`, then add via Settings UI again

2. **System Env Var Not Detected**:
   - Solution: Make sure you set the env var BEFORE launching the app
   - Or restart your entire terminal/PowerShell session

3. **Still Getting Hardcoded Responses**:
   - Check the console logs for errors
   - Verify the provider dropdown shows "OpenAI" or "Anthropic", not "Local"
   - Make sure your API key is valid (test it at platform.openai.com or console.anthropic.com)

4. **"Unsupported parameter" Errors (GPT-5 models)**:
   - GPT-5 models have breaking API changes from GPT-4:
     - ✅ Use `max_completion_tokens` (not `max_tokens`)
     - ✅ Don't support custom `temperature` (only default 1.0)
   - The app now automatically handles these differences based on model version
   - If you see parameter errors, make sure you're using the latest code

5. **"Sorry, I encountered an issue generating a response" (Empty Response)**:
   - **Check console logs** - Look for `[callOpenAI] Response received:` or `[callAnthropic] Response received:`
   - **If `finishReason: 'length'` with `hasContent: false`**:
     - This means the token limit was hit BEFORE any content was generated
     - Fixed: Token limits increased to 8192 (GPT-5) / 4096 (GPT-4/Claude)
     - If still happening, your system context + user message is too large
     - Try a shorter/simpler question
   - **If `hasContent: false` with other finish reasons**:
     - API is responding but returning no text
     - Usually means invalid/expired API key or model access issue
     - Verify your key at platform.openai.com or console.anthropic.com
   - **If you see multiple calls (up to 3x)**:
     - That's normal - it's the reflection system trying to improve the response
     - If all 3 fail with the same error, check your API key and model access
   - **If you don't see API calls at all**:
     - The app might be falling back to local mode
     - Check for `[LLM] No API key found for...` earlier in the logs

6. **Multiple System Messages Error**:
   - Fixed: Reflections now use 'user' role messages, not 'system'
   - OpenAI/Anthropic only support one system message at the start
   - If you see errors about message format, clear your browser cache and restart

### Testing with Fake Key:

If you want to test the API key detection without spending credits:
1. Add a fake key (e.g., `sk-test123`) via Settings UI
2. Send a message
3. Console should show: "Found (sk-test1...)"
4. You'll get an API error instead of a hardcoded response, proving the LLM is being called
