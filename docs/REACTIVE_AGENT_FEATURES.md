# Reactive Agent Features

The PiBeat agent is now fully reactive with advanced capabilities to handle complex tasks, token limits, and iterative refinement.

## Key Features

### 1. **Automatic Token Limit Handling**
When the AI's response is truncated due to token limits, the agent automatically:
- Detects truncation (`finishReason: 'length'` for OpenAI, `stop_reason: 'max_tokens'` for Anthropic)
- Continues generating from where it left off
- Accumulates multi-part responses seamlessly
- Supports up to 3 continuation cycles per response

**Example:**
```
User: "Generate a complete 5-minute Sonic Pi track with drums, bass, melody, and pads"
Agent: [Generates part 1] → [Detects truncation] → [Continues with part 2] → [Combines results]
```

### 2. **Task Splitting for Complex Requests**
The agent intelligently detects when you ask for multiple things and splits them:
- Recognizes numbered lists (1. drums, 2. bass, 3. melody)
- Detects multiple requests connected by "and", "also", "plus", "additionally"
- Processes each subtask independently
- Combines results with clear section markers

**Example:**
```
User: "Generate a kick drum pattern AND a snare pattern AND a hihat pattern"
Agent: 
### Part 1: Generate a kick drum pattern
[code here]

---

### Part 2: Generate a snare pattern
[code here]

---

### Part 3: Generate a hihat pattern
[code here]
```

### 3. **Iterative Reflection & Self-Improvement**
The agent evaluates its own responses and refines them:
- Checks for unclosed code blocks → automatically continues
- Detects missing code when user asks for code → regenerates
- Identifies incomplete markers (`...`, `TODO`, `FIXME`) → completes them
- Recognizes sparse responses for complex tasks → splits and retries
- Maximum 3 reflection cycles per request

**Quality Checks:**
- ✅ Code blocks properly opened and closed
- ✅ Complete code without placeholders
- ✅ Adequate response length for context
- ✅ All requested elements present
- ✅ No error indicators in short responses

### 4. **Smart Context Management**
To prevent hitting context limits in long conversations:
- Automatically truncates old messages (keeps last 10 messages)
- Maintains system context with Sonic Pi knowledge
- Preserves recent conversation for relevance
- Logs truncation actions for debugging

### 5. **Retry Logic with Exponential Backoff**
Handles transient errors gracefully:
- Automatic retry up to 2 times on failures
- Exponential backoff (1s, 2s delays)
- Skips retry for authentication errors (401)
- Skips retry for rate limit errors (429)

## How It Works

### Reactive Agent Process Flow

```
1. User sends message
   ↓
2. Context management (truncate if too long)
   ↓
3. Initial LLM call
   ↓
4. Check if truncated → Continue generating if needed
   ↓
5. Reflection: Evaluate response quality
   ↓
6. If needs improvement:
   - Unclosed code block? → Continue
   - Missing code? → Regenerate
   - Complex task? → Split into subtasks
   - Incomplete? → Extend response
   ↓
7. Return final accumulated response
```

### Token Limit Handling Example

**Scenario:** System context is 4920 chars (~1230 tokens), user asks for large code example.

**Without token handling:**
```
Response: "Here's a drum pattern:
```ruby
live_loop :kick do
  sample :..."
[TRUNCATED - response incomplete]
```

**With reactive agent:**
```
Response Part 1: "Here's a drum pattern:
```ruby
live_loop :kick do
  sample :bd_haus
  sleep 1
end
```

[Agent detects truncation]

Response Part 2 (continuation): "
live_loop :snare do
  sleep 0.5
  sample :sn_dub
  sleep 0.5
end
```

[Final combined response shows complete code]
```

## Configuration

The reactive agent behavior is configured in `src/llm.ts`:

```typescript
export async function reactiveAgentProcess(
  config: LLMConfig,
  context: AgentContext
): Promise<AgentMessage> {
  const maxReflections = config.maxReflections ?? 3; // Max self-improvement cycles
  const maxContinuations = 3; // Max times to continue truncated responses
  const maxHistoryMessages = 10; // Max conversation history to keep
  // ...
}
```

You can adjust these values for different trade-offs:
- **Higher maxReflections** → Better quality, slower responses
- **Higher maxContinuations** → Longer responses possible, more API calls
- **Higher maxHistoryMessages** → Better context awareness, larger token usage

## Usage Tips

### For Best Results

1. **Be specific:** "Generate a 16-beat drum pattern with kick on 1, 5, 9, 13"
2. **Break complex tasks naturally:** The agent will auto-split, but clear structure helps
3. **Use numbered lists:** "1. Create drums 2. Add bass 3. Add melody"
4. **Check console logs:** F12 shows detailed agent reasoning and token usage

### Console Log Examples

**Token Limit Handling:**
```
[reactiveAgentProcess] Initial response: { length: 2847, truncated: true, finishReason: 'length' }
[reactiveAgentProcess] Response truncated, continuing... (1/3)
[reactiveAgentProcess] Continuation response: { length: 1523, totalLength: 4370, truncated: false }
```

**Task Splitting:**
```
[handleTaskSplitting] Identified 3 subtasks: ["Create a kick pattern", "Create a snare pattern", "Create a hihat pattern"]
[handleTaskSplitting] Processing subtask 1/3: Create a kick pattern...
[handleTaskSplitting] Processing subtask 2/3: Create a snare pattern...
[handleTaskSplitting] Processing subtask 3/3: Create a hihat pattern...
```

**Reflection:**
```
[reactiveAgentProcess] Reflection cycle 1/3
[reactiveAgentProcess] Reflection result: {
  needsMoreInfo: true,
  action: 'continue',
  thought: 'Code block is not properly closed'
}
```

## Technical Details

### LLMResponse Type
```typescript
export interface LLMResponse {
  content: string;           // The actual response text
  truncated: boolean;        // Was it cut off due to token limit?
  finishReason?: string;     // 'stop', 'length', 'max_tokens', etc.
  usage?: {
    promptTokens: number;    // Tokens in the input
    completionTokens: number;// Tokens in the output
    totalTokens: number;     // Total tokens used
  };
}
```

### Reflection Actions
- `'continue'` - Extend current response (unclosed code, incomplete)
- `'generate'` - Create new response (missing code)
- `'refactor'` - Improve existing code
- `'split_task'` - Break into subtasks
- `'analyze_code'` - Deep dive into code
- `'done'` - Response is complete

## Troubleshooting

### Agent keeps iterating but not improving
- Check console logs for reflection reasoning
- Adjust `maxReflections` if needed
- Verify response quality checks are appropriate for your use case

### Responses are being split unnecessarily
- Avoid connecting words in unrelated parts of your message
- Use clearer phrasing that doesn't trigger multi-request detection
- Check `[handleTaskSplitting]` logs to see what triggered splitting

### Token limits still causing issues
- Check `max_completion_tokens` / `max_tokens` in `callOpenAI` / `callAnthropic`
- Verify continuation logic is working (look for `[reactiveAgentProcess] Response truncated` logs)
- Consider simplifying system context if needed

## Performance Metrics

Typical token usage with reactive agent:

| Scenario | API Calls | Total Tokens | Time |
|----------|-----------|--------------|------|
| Simple question | 1 | 1,500-2,000 | 2-3s |
| Code generation | 1-2 | 3,000-5,000 | 4-6s |
| Complex multi-part | 2-4 | 6,000-12,000 | 8-15s |
| Truncated response | 2-3 | 8,000-16,000 | 10-18s |

## Future Enhancements

Potential improvements for v2:
- [ ] LLM-based reflection (use AI to evaluate its own responses)
- [ ] Parallel subtask execution (faster multi-part responses)
- [ ] Adaptive token allocation (adjust limits based on task complexity)
- [ ] Response caching (avoid regenerating similar requests)
- [ ] User feedback loop (learn from "Insert" vs "Reject" actions)

---

**Note:** All these features work with both OpenAI (GPT-5/GPT-4) and Anthropic (Claude) models. The agent automatically adapts to each provider's API requirements.
