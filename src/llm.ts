/**
 * LLM Integration — Reactive agent with OpenAI and Anthropic support
 * 
 * This module provides:
 * - Multi-provider LLM support (OpenAI, Anthropic)
 * - Reactive agent pattern with reflection and multi-turn reasoning
 * - Sonic Pi knowledge injection as system context
 * - Fallback to local pattern-matching when no API key available
 */

import OpenAI from 'openai';
import Anthropic from '@anthropic-ai/sdk';
import { invoke } from '@tauri-apps/api/core';
import { AgentMessage } from './store';

// ──────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────

export type LLMProvider = 'openai' | 'anthropic' | 'local';
export type ModelId = 
  | 'gpt-5.2' 
  | 'gpt-5-mini'
  | 'gpt-5-nano'
  | 'gpt-4o' 
  | 'gpt-4o-mini'
  | 'claude-sonnet-4.5'
  | 'claude-3-5-sonnet-20241022'
  | 'claude-3-5-haiku-20241022'
  | 'local-rules';

export interface LLMConfig {
  provider: LLMProvider;
  model: ModelId;
  apiKey?: string;
  maxReflections?: number; // Max number of self-reflection iterations
}

export interface ReflectionResult {
  needsMoreInfo: boolean;
  thought: string;
  action?: 'ask_user' | 'analyze_code' | 'generate' | 'refactor' | 'continue' | 'split_task' | 'done';
}

export interface LLMResponse {
  content: string;
  truncated: boolean;
  finishReason?: string;
  usage?: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
}

// ──────────────────────────────────────────────
// Model configurations
// ──────────────────────────────────────────────

export const AVAILABLE_MODELS: Record<LLMProvider, { id: ModelId; name: string }[]> = {
  openai: [
    { id: 'gpt-5.2', name: 'GPT-5.2 (Latest & Best)' },
    { id: 'gpt-5-mini', name: 'GPT-5 Mini (Fast & Smart)' },
    { id: 'gpt-5-nano', name: 'GPT-5 Nano (Ultra Fast)' },
    { id: 'gpt-4o', name: 'GPT-4o (Legacy)' },
    { id: 'gpt-4o-mini', name: 'GPT-4o Mini (Legacy)' },
  ],
  anthropic: [
    { id: 'claude-sonnet-4.5', name: 'Claude Sonnet 4.5 (Latest & Best)' },
    { id: 'claude-3-5-sonnet-20241022', name: 'Claude 3.5 Sonnet (Legacy)' },
    { id: 'claude-3-5-haiku-20241022', name: 'Claude 3.5 Haiku (Fast)' },
  ],
  local: [
    { id: 'local-rules', name: 'Local (Rule-based, No API)' },
  ],
};

// ──────────────────────────────────────────────
// Knowledge Base — Injected as system context
// ──────────────────────────────────────────────

const SONIC_PI_SYSTEM_CONTEXT = `You are an expert Sonic Pi coding assistant embedded in PiBeat, a music coding application. You have deep knowledge of Sonic Pi syntax and can help users write, refactor, and understand music code.

## Your Capabilities
- Generate drum beats, melodies, arps, basslines, pads, and full multi-layer tracks
- Refactor and improve existing Sonic Pi code
- Explain code line-by-line
- Analyze code for issues (missing sleep, infinite loops, etc.)
- Suggest effects and parameters
- Answer questions about Sonic Pi syntax

## PiBeat Application Context
You're integrated into a Tauri-based desktop app with:
- **Multiple buffers (0-9)** — like Sonic Pi's buffer tabs
- **Live code execution** — users can run code with Alt+R, stop with Alt+S
- **Monaco editor** with Sonic Pi syntax highlighting
- **Real-time waveform visualization**
- **Sample browser** with kick, snare, hihat, clap, loops, ambient sounds
- **Effects panel** with global reverb, delay, distortion, filters
- **You can see the user's current buffer code** when they ask questions

## Sonic Pi Language Reference

### Core Syntax
\`\`\`ruby
play :c4              # Play middle C
play 60               # MIDI note number
sleep 0.5             # Wait half a beat
sample :kick          # Play a sample
use_synth :saw        # Change synth
use_bpm 120           # Set tempo
\`\`\`

### Available Synths
:sine, :beep, :saw, :dsaw, :square, :tri, :triangle, :noise, :pulse, :super_saw, :tb303, :prophet, :blade, :pluck, :fm, :mod_fm, :mod_saw, :mod_pulse, :mod_tri

### Available Samples
:kick, :snare, :hihat, :clap, :bass, :perc, :loop_amen, :loop_breakbeat, :ambi_choir, :ambi_dark_woosh, :ambi_drone, :bd_ada, :bd_boom, :bd_808, :drum_bass_hard, :drum_heavy_kick, :drum_snare_soft, :elec_beep, :elec_blip, :misc_cineboom

### Effects
:reverb, :echo, :delay, :distortion, :lpf, :hpf, :flanger, :slicer, :wobble, :compressor, :pitch_shift, :ring_mod, :bitcrusher

Usage: \`with_fx :reverb, mix: 0.5, room: 0.8 do ... end\`

### Live Loops (Essential for PiBeat)
\`\`\`ruby
live_loop :drums do
  sample :kick
  sleep 0.5
  sample :hihat
  sleep 0.5
end
\`\`\`
**Critical:** Always include \`sleep\` inside \`live_loop\` — without it, the loop locks up!

### Common Parameters
- \`amp:\` volume (0.0-1.0+)
- \`pan:\` stereo (-1 to 1)
- \`attack:\`, \`decay:\`, \`sustain:\`, \`release:\` envelope
- \`cutoff:\` filter cutoff (0-130)
- \`res:\` filter resonance (0.0-1.0)
- \`rate:\` sample playback speed

### Chords & Scales
\`\`\`ruby
play chord(:c4, :major)                        # C E G
play_pattern_timed scale(:c4, :minor), [0.25]  # C D Eb F G Ab Bb C
\`\`\`

### Rings & Ticks (Idiomatic Patterns)
\`\`\`ruby
notes = ring(:c4, :e4, :g4)
live_loop :arp do
  play notes.tick  # Cycles: c4, e4, g4, c4, ...
  sleep 0.25
end
\`\`\`

### Randomization
\`\`\`ruby
use_random_seed 42           # Reproducible
play rrand_i(50, 80)         # Random MIDI note
play choose([:c4, :e4, :g4]) # Pick one
\`\`\`

### Threads & Sync
\`\`\`ruby
in_thread do
  loop do
    sample :kick
    sleep 1
  end
end

# Coordination:
in_thread do
  sync :go
  play :c4
end
cue :go  # Triggers the waiting thread
\`\`\`

## Code Generation Guidelines
When generating code:
1. **Always use \`live_loop\`** — enables hot-reloading and continuous playback
2. **Every loop MUST have \`sleep\`** — critical to avoid lockups
3. **Use meaningful loop names** — :drums, :bass, :melody, :pad
4. **Prefer \`ring\` with \`.tick\`** over manual indexing
5. **Add \`use_bpm\` at the top** to make tempo explicit
6. **Layer multiple \`live_loop\` blocks** for rich compositions
7. **Use \`with_fx\`** to add depth (reverb, echo, filters)
8. **Parameterize with variables** for easy tweaking
9. **Use sensible \`amp\` values** — 0.3-0.8 for most elements
10. **Add comments** to explain patterns

## Refactoring Principles
When improving code:
- Extract repeated patterns into \`define\` functions
- Convert \`loop\` to \`live_loop\` for hot-reload support
- Use \`ring\` and \`.tick\` instead of manual arrays
- Break long monolithic code into multiple \`live_loop\` blocks
- Add missing \`sleep\` statements
- Suggest \`use_random_seed\` for reproducible randomness
- Use \`spread\` for euclidean rhythms: \`(spread 5, 8)\`

## Response Format
When the user asks you to generate code:
- Provide complete, runnable code wrapped in \`\`\`ruby code blocks
- Explain what the code does briefly
- Suggest variations or parameters they can tweak

When refactoring:
- Show the refactored code
- List the changes you made and why

When explaining:
- Go line-by-line or concept-by-concept
- Use simple language
- Relate to musical concepts where helpful

## User Interaction
The user can:
- Type natural language requests ("make a beat", "add reverb to my code")
- Ask you to analyze their current buffer
- Request explanations, refactorings, or new code snippets
- Insert your code suggestions into their buffer or replace the entire buffer

Be concise, helpful, and focused on making great music!`;

// ──────────────────────────────────────────────
// LLM Clients
// ──────────────────────────────────────────────

let openaiClient: OpenAI | null = null;
let anthropicClient: Anthropic | null = null;

function getOpenAIClient(apiKey: string): OpenAI {
  if (!openaiClient || openaiClient.apiKey !== apiKey) {
    openaiClient = new OpenAI({ apiKey, dangerouslyAllowBrowser: true });
  }
  return openaiClient;
}

function getAnthropicClient(apiKey: string): Anthropic {
  if (!anthropicClient || anthropicClient.apiKey !== apiKey) {
    anthropicClient = new Anthropic({ apiKey, dangerouslyAllowBrowser: true });
  }
  return anthropicClient;
}

// ──────────────────────────────────────────────
// Reactive Agent with Reflection
// ──────────────────────────────────────────────

interface AgentContext {
  currentCode: string;
  conversationHistory: AgentMessage[];
  userMessage: string;
}

/**
 * Main reactive agent — can reflect on its own responses and iterate
 * Now with token limit handling, task splitting, and better iteration
 */
export async function reactiveAgentProcess(
  config: LLMConfig,
  context: AgentContext
): Promise<AgentMessage> {
  const maxReflections = config.maxReflections ?? 3; // Increased to 3 for better iteration
  
  // If local mode, use rule-based agent
  if (config.provider === 'local') {
    console.log('[LLM] Using local rule-based agent');
    const { processAgentMessage } = await import('./agent');
    return processAgentMessage(context.userMessage, context.currentCode, context.conversationHistory);
  }

  // Check for API key from all sources if not provided or empty
  let apiKey = config.apiKey && config.apiKey.trim() ? config.apiKey.trim() : undefined;
  
  if (!apiKey && (config.provider === 'openai' || config.provider === 'anthropic')) {
    console.log(`[LLM] No API key in config, checking environment/storage for ${config.provider}...`);
    const envApiKey = await getApiKey(config.provider);
    apiKey = envApiKey && envApiKey.trim() ? envApiKey.trim() : undefined;
    console.log(`[LLM] API key from environment/storage: ${apiKey ? 'Found (' + apiKey.substring(0, 8) + '...)' : 'Not found'}`);
  } else if (apiKey) {
    console.log(`[LLM] Using API key from config: ${apiKey.substring(0, 8)}...`);
  }

  // Fall back to rule-based if no API key found
  if (!apiKey) {
    console.warn(`[LLM] No API key found for ${config.provider}, falling back to local mode`);
    const { processAgentMessage } = await import('./agent');
    return processAgentMessage(context.userMessage, context.currentCode, context.conversationHistory);
  }

  console.log(`[LLM] Calling ${config.provider} with model ${config.model}`);

  // Create new config with resolved API key
  const resolvedConfig = { ...config, apiKey };

  // Manage context to avoid hitting limits
  const managedContext = manageContext(context);

  let accumulatedResponse = '';
  let reflectionCount = 0;
  let continueCount = 0;
  const maxContinuations = 3; // Max times to continue a truncated response

  try {
    // Initial generation
    console.log('[reactiveAgentProcess] Making initial LLM call...');
    let response = await callLLMWithRetry(resolvedConfig, managedContext, null);
    accumulatedResponse = response.content;
    console.log('[reactiveAgentProcess] Initial response:', {
      length: response.content?.length || 0,
      truncated: response.truncated,
      finishReason: response.finishReason
    });

    // Handle token limit truncation — continue generating if cut off
    while (response.truncated && continueCount < maxContinuations) {
      console.log(`[reactiveAgentProcess] Response truncated, continuing... (${continueCount + 1}/${maxContinuations})`);
      
      // Add the partial response to context and ask to continue
      const continueContext: AgentContext = {
        ...managedContext,
        conversationHistory: [
          ...managedContext.conversationHistory,
          { role: 'assistant', content: accumulatedResponse },
          { role: 'user', content: 'Please continue from where you left off. Complete the code or explanation.' }
        ]
      };
      
      response = await callLLMWithRetry(resolvedConfig, continueContext, null);
      accumulatedResponse += '\n' + response.content;
      continueCount++;
      
      console.log('[reactiveAgentProcess] Continuation response:', {
        length: response.content?.length || 0,
        totalLength: accumulatedResponse.length,
        truncated: response.truncated
      });
    }

    // Reflection loop — agent evaluates its own response and improves if needed
    while (reflectionCount < maxReflections) {
      console.log(`[reactiveAgentProcess] Reflection cycle ${reflectionCount + 1}/${maxReflections}`);
      const reflection = await reflectOnResponse(resolvedConfig, managedContext, accumulatedResponse);
      
      console.log(`[reactiveAgentProcess] Reflection result:`, {
        needsMoreInfo: reflection.needsMoreInfo,
        action: reflection.action,
        thought: reflection.thought,
      });
      
      if (!reflection.needsMoreInfo || reflection.action === 'done') {
        console.log('[reactiveAgentProcess] Reflection complete, response is good');
        break; // Response is good, stop reflecting
      }

      // Handle task splitting if needed
      if (reflection.action === 'split_task') {
        console.log('[reactiveAgentProcess] Splitting complex task...');
        const splitResponse = await handleTaskSplitting(resolvedConfig, managedContext, accumulatedResponse);
        accumulatedResponse = splitResponse;
        break; // Task splitting completes the process
      }

      // Agent determined it needs to improve — make another LLM call
      console.log('[reactiveAgentProcess] Making reflection improvement call...');
      response = await callLLMWithRetry(resolvedConfig, managedContext, reflection);
      
      // If reflection asks to continue/extend, append; otherwise replace
      if (reflection.action === 'continue') {
        accumulatedResponse += '\n\n' + response.content;
      } else {
        accumulatedResponse = response.content;
      }
      
      console.log('[reactiveAgentProcess] Improved response length:', accumulatedResponse.length);
      reflectionCount++;
    }

    console.log('[reactiveAgentProcess] Final response length:', accumulatedResponse?.length || 0);
    
    return {
      role: 'assistant',
      content: accumulatedResponse || 'Sorry, I received an empty response from the AI.',
    };
  } catch (error: any) {
    console.error('[reactiveAgentProcess] Error during agent process:', error);
    throw error; // Propagate to AgentChat error handler
  }
}

/**
 * Make a single LLM call with full context and retry logic
 */
async function callLLMWithRetry(
  config: LLMConfig,
  context: AgentContext,
  reflection: ReflectionResult | null,
  retries: number = 2
): Promise<LLMResponse> {
  let lastError: any;
  
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      if (attempt > 0) {
        console.log(`[callLLMWithRetry] Retry attempt ${attempt}/${retries}`);
        await new Promise(resolve => setTimeout(resolve, 1000 * attempt)); // Exponential backoff
      }
      
      const messages = buildMessages(context, reflection);
      
      console.log('[callLLMWithRetry] Built messages:', {
        provider: config.provider,
        messageCount: messages.length,
        roles: messages.map(m => m.role),
        systemMessageLength: messages.find(m => m.role === 'system')?.content.length || 0,
        lastMessagePreview: messages[messages.length - 1]?.content.substring(0, 100),
      });

      if (config.provider === 'openai') {
        return await callOpenAI(config, messages);
      } else if (config.provider === 'anthropic') {
        return await callAnthropic(config, messages);
      }

      throw new Error(`Unsupported provider: ${config.provider}`);
    } catch (error: any) {
      lastError = error;
      
      // Don't retry on authentication errors
      if (error.message?.includes('401') || error.message?.includes('invalid_api_key')) {
        throw error;
      }
      
      // Don't retry on rate limit errors (should be handled differently)
      if (error.message?.includes('429') || error.message?.includes('rate_limit')) {
        throw error;
      }
      
      console.warn(`[callLLMWithRetry] Attempt ${attempt + 1} failed:`, error.message);
    }
  }
  
  throw lastError;
}

/**
 * Manage context size to avoid token limits
 * Keeps system context + recent messages
 */
function manageContext(context: AgentContext): AgentContext {
  const maxHistoryMessages = 10; // Keep last 10 messages
  
  if (context.conversationHistory.length <= maxHistoryMessages) {
    return context; // Small history, no need to truncate
  }
  
  console.log(`[manageContext] Truncating history from ${context.conversationHistory.length} to ${maxHistoryMessages} messages`);
  
  // Keep most recent messages
  const recentHistory = context.conversationHistory.slice(-maxHistoryMessages);
  
  return {
    ...context,
    conversationHistory: recentHistory
  };
}

/**
 * Handle complex tasks by splitting them into subtasks
 */
async function handleTaskSplitting(
  config: LLMConfig,
  context: AgentContext,
  currentResponse: string
): Promise<string> {
  console.log('[handleTaskSplitting] Analyzing task complexity...');
  
  // Detect if user asked for multiple things
  const hasMultipleRequests = /\band\b|\balso\b|\badditionally\b|\bplus\b/gi.test(context.userMessage);
  const hasNumberedList = /\b\d+[.)].+/g.test(context.userMessage);
  
  if (!hasMultipleRequests && !hasNumberedList) {
    console.log('[handleTaskSplitting] Task not complex enough to split, returning current response');
    return currentResponse;
  }
  
  // Extract subtasks using simple heuristics
  const subtasks: string[] = [];
  
  if (hasNumberedList) {
    // Extract numbered items
    const matches = context.userMessage.match(/\b\d+[.)]\s*([^\n]+)/g);
    if (matches) {
      subtasks.push(...matches.map(m => m.replace(/^\d+[.)]\s*/, '')));
    }
  } else {
    // Split by 'and', 'also', etc.
    const parts = context.userMessage.split(/\band\b|\balso\b|\badditionally\b|\bplus\b/gi);
    subtasks.push(...parts.filter(p => p.trim().length > 10));
  }
  
  console.log(`[handleTaskSplitting] Identified ${subtasks.length} subtasks:`, subtasks);
  
  if (subtasks.length < 2) {
    return currentResponse; // Not enough subtasks
  }
  
  // Process each subtask
  const results: string[] = [];
  
  for (let i = 0; i < Math.min(subtasks.length, 5); i++) { // Max 5 subtasks
    console.log(`[handleTaskSplitting] Processing subtask ${i + 1}/${subtasks.length}: ${subtasks[i].substring(0, 50)}...`);
    
    const subtaskContext: AgentContext = {
      ...context,
      userMessage: subtasks[i],
      conversationHistory: [] // Fresh context for each subtask
    };
    
    try {
      const response = await callLLMWithRetry(config, subtaskContext, null);
      results.push(`### Part ${i + 1}: ${subtasks[i].substring(0, 60)}...\n\n${response.content}`);
    } catch (error) {
      console.error(`[handleTaskSplitting] Failed on subtask ${i + 1}:`, error);
      results.push(`### Part ${i + 1}: ${subtasks[i].substring(0, 60)}...\n\n_Could not complete this part._`);
    }
  }
  
  return results.join('\n\n---\n\n');
}

/**
 * Build message array for LLM call
 */
function buildMessages(
  context: AgentContext,
  reflection: ReflectionResult | null
): Array<{ role: 'system' | 'user' | 'assistant'; content: string }> {
  const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [
    { role: 'system', content: SONIC_PI_SYSTEM_CONTEXT },
  ];

  // Add conversation history
  for (const msg of context.conversationHistory) {
    messages.push({
      role: msg.role === 'user' ? 'user' : 'assistant',
      content: msg.content,
    });
  }

  // Add current user message with code context
  const userPrompt = buildUserPrompt(context.userMessage, context.currentCode);
  messages.push({ role: 'user', content: userPrompt });

  // If we're in a reflection loop, add the reflection as a user instruction
  // (OpenAI/Anthropic don't support multiple system messages)
  if (reflection) {
    messages.push({
      role: 'user',
      content: `Please improve your previous response. Reflection: ${reflection.thought}\nFocus on: ${reflection.action || 'improving quality'}`,
    });
  }

  return messages;
}

function buildUserPrompt(userMessage: string, currentCode: string): string {
  if (currentCode.trim().length > 10) {
    return `${userMessage}\n\n[Current buffer code]:\n\`\`\`ruby\n${currentCode}\n\`\`\``;
  }
  return userMessage;
}

/**
 * OpenAI API call with enhanced response handling
 */
async function callOpenAI(
  config: LLMConfig,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>
): Promise<LLMResponse> {
  const client = getOpenAIClient(config.apiKey!);
  
  // GPT-5 models have different API requirements than GPT-4
  const isGPT5 = config.model.startsWith('gpt-5');
  const completionParams: any = {
    model: config.model,
    messages,
  };

  if (isGPT5) {
    // GPT-5: use max_completion_tokens, no custom temperature (only default 1 supported)
    // GPT-5 models support up to 16K output tokens, but we limit to 8192 for reasonable response times
    completionParams.max_completion_tokens = 8192;
    // Omit temperature - GPT-5 only supports default (1)
  } else {
    // GPT-4: use max_tokens and allow temperature customization
    completionParams.max_tokens = 4096; // Increased from 2000
    completionParams.temperature = 0.7;
  }
  
  console.log(`[callOpenAI] Calling with params:`, { 
    model: completionParams.model,
    hasMaxTokens: !!completionParams.max_tokens,
    hasMaxCompletionTokens: !!completionParams.max_completion_tokens,
    temperature: completionParams.temperature 
  });
  
  try {
    const response = await client.chat.completions.create(completionParams);
    
    // Log detailed response structure
    const firstChoice = response.choices?.[0];
    const message = firstChoice?.message;
    
    console.log('[callOpenAI] Response received:', {
      id: response.id,
      model: response.model,
      choices: response.choices?.length || 0,
      hasMessage: !!message,
      messageRole: message?.role,
      hasContent: !!message?.content,
      contentType: typeof message?.content,
      contentLength: message?.content?.length || 0,
      finishReason: firstChoice?.finish_reason,
    });

    const content = message?.content || '';
    const truncated = firstChoice?.finish_reason === 'length';
    
    if (!content || content.trim() === '') {
      console.error('[callOpenAI] No content in response.');
      console.error('[callOpenAI] Full message object:', message);
      console.error('[callOpenAI] Full response:', response);
      
      // Check if it was cut off due to length
      if (truncated) {
        throw new Error('Response was truncated before generating content. This is unusual - check your API key and model access.');
      }
      
      throw new Error('OpenAI returned empty response');
    }
    
    // Log if response was truncated
    if (truncated) {
      console.warn('[callOpenAI] Response was truncated at token limit, will continue in next call');
    }
    
    console.log('[callOpenAI] Successfully got content, length:', content.length);
    
    return {
      content,
      truncated,
      finishReason: firstChoice?.finish_reason,
      usage: {
        promptTokens: response.usage?.prompt_tokens || 0,
        completionTokens: response.usage?.completion_tokens || 0,
        totalTokens: response.usage?.total_tokens || 0,
      }
    };
  } catch (error: any) {
    console.error('[callOpenAI] Error:', error);
    throw error; // Re-throw to be handled by caller
  }
}

/**
 * Anthropic API call with enhanced response handling
 */
async function callAnthropic(
  config: LLMConfig,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>
): Promise<LLMResponse> {
  const client = getAnthropicClient(config.apiKey!);

  // Anthropic expects system message separate from messages array
  const systemMessage = messages.find(m => m.role === 'system')?.content || '';
  const chatMessages = messages
    .filter(m => m.role !== 'system')
    .map(m => ({
      role: m.role as 'user' | 'assistant',
      content: m.content,
    }));

  console.log(`[callAnthropic] Calling with model: ${config.model}`);

  try {
    const response = await client.messages.create({
      model: config.model,
      system: systemMessage,
      messages: chatMessages,
      max_tokens: 4096, // Anthropic uses higher limit
      temperature: 1.0, // Anthropic default is 1.0
    });

    console.log('[callAnthropic] Response received:', {
      id: response.id,
      model: response.model,
      contentBlocks: response.content?.length || 0,
      stopReason: response.stop_reason,
    });

    const textBlock = response.content.find(block => block.type === 'text');
    
    if (!textBlock || textBlock.type !== 'text') {
      console.error('[callAnthropic] No text content in response. Full response:', response);
      throw new Error('Anthropic returned empty response');
    }
    
    const content = textBlock.text || '';
    const truncated = response.stop_reason === 'max_tokens';
    
    if (truncated) {
      console.warn('[callAnthropic] Response was truncated at token limit, will continue in next call');
    }
    
    return {
      content,
      truncated,
      finishReason: response.stop_reason || undefined,
      usage: {
        promptTokens: response.usage?.input_tokens || 0,
        completionTokens: response.usage?.output_tokens || 0,
        totalTokens: (response.usage?.input_tokens || 0) + (response.usage?.output_tokens || 0),
      }
    };
  } catch (error: any) {
    console.error('[callAnthropic] Error:', error);
    throw error; // Re-throw to be handled by caller
  }
}

/**
 * Reflection — Agent evaluates its own response and decides if it needs improvement
 * Enhanced with better quality checks
 */
async function reflectOnResponse(
  _config: LLMConfig,
  context: AgentContext,
  response: string
): Promise<ReflectionResult> {
  // Enhanced heuristic-based reflection
  
  // Check if response contains code blocks
  const codeBlockMatches = response.match(/```/g);
  const hasCodeBlock = !!codeBlockMatches;
  const hasUnclosedCodeBlock = codeBlockMatches && codeBlockMatches.length % 2 !== 0;
  
  // Check if user asked for code generation
  const userWantsCode = /generate|create|make|build|write|show me|give me|example/i.test(context.userMessage);
  
  // Check if response is too short
  const isTooShort = response.length < 100;
  
  // User asked to refactor but response doesn't show refactored code
  const askedRefactor = /refactor|improve|clean|optimize/i.test(context.userMessage);
  const hasRefactoredCode = hasCodeBlock && response.includes('```');
  
  // Check for incomplete code patterns
  const hasIncompleteCode = /\.\.\.\s*$|# \.\.\./.test(response) || /TODO|FIXME|incomplete/i.test(response);
  
  // Check for error indicators
  const hasErrorIndicators = /error|exception|failed|cannot/i.test(response) && response.length < 300;
  
  // Detect if task seems complex and might need splitting
  const hasMultipleRequests = /\band\b.*\band\b/gi.test(context.userMessage) || /\d+[.)].*\d+[.)]/s.test(context.userMessage);
  const responseSeemsSparse = hasMultipleRequests && response.length < 500;

  // Check for unclosed code blocks
  if (hasUnclosedCodeBlock) {
    return {
      needsMoreInfo: true,
      thought: 'Code block is not properly closed',
      action: 'continue',
    };
  }

  // Check for incomplete code
  if (hasIncompleteCode) {
    return {
      needsMoreInfo: true,
      thought: 'Response contains incomplete code markers',
      action: 'continue',
    };
  }

  if (userWantsCode && !hasCodeBlock) {
    return {
      needsMoreInfo: true,
      thought: 'User asked for code but response lacks code block',
      action: 'generate',
    };
  }

  if (askedRefactor && !hasRefactoredCode) {
    return {
      needsMoreInfo: true,
      thought: 'User asked to refactor but no refactored code provided',
      action: 'refactor',
    };
  }

  if (isTooShort && context.currentCode.length > 50) {
    return {
      needsMoreInfo: true,
      thought: 'Response is too brief given the context',
      action: 'analyze_code',
    };
  }
  
  if (hasErrorIndicators) {
    return {
      needsMoreInfo: true,
      thought: 'Response seems to indicate an error, trying again',
      action: 'generate',
    };
  }
  
  if (responseSeemsSparse) {
    return {
      needsMoreInfo: true,
      thought: 'User requested multiple things, response seems sparse',
      action: 'split_task',
    };
  }

  // Response looks good
  return {
    needsMoreInfo: false,
    thought: 'Response is complete and adequate',
    action: 'done',
  };
}

// ──────────────────────────────────────────────
// API Key Management
// ──────────────────────────────────────────────

/**
 * Get API key from system environment variables (via Tauri) or localStorage
 * Priority: 
 * 1. System environment variables (OPENAI_API_KEY, ANTHROPIC_API_KEY) - checked via Tauri
 * 2. Vite .env file variables (import.meta.env.*)
 * 3. localStorage (set via settings UI)
 */
export async function getApiKey(provider: 'openai' | 'anthropic'): Promise<string | null> {
  const envVarName = provider === 'openai' ? 'OPENAI_API_KEY' : 'ANTHROPIC_API_KEY';
  
  console.log(`[getApiKey] Checking for ${envVarName}...`);
  
  // 1. Check system environment variables via Tauri
  try {
    const systemEnvKey = await invoke<string | null>('get_env_var', { key: envVarName });
    if (systemEnvKey && systemEnvKey.trim().length > 0) {
      console.log(`[getApiKey] Found in system env: ${systemEnvKey.substring(0, 8)}...`);
      return systemEnvKey.trim();
    }
    console.log('[getApiKey] Not found in system env');
  } catch (error) {
    console.warn(`[getApiKey] Failed to read system env var ${envVarName}:`, error);
  }

  // 2. Check Vite .env file variables (build-time only)
  const viteEnvKey = import.meta.env[envVarName];
  if (viteEnvKey && typeof viteEnvKey === 'string' && viteEnvKey.trim().length > 0) {
    console.log(`[getApiKey] Found in Vite .env: ${viteEnvKey.substring(0, 8)}...`);
    return viteEnvKey.trim();
  }
  console.log('[getApiKey] Not found in Vite .env');

  // 3. Fall back to localStorage (user configured via settings modal)
  const storageKey = getStoredApiKey(provider);
  if (storageKey && storageKey.trim().length > 0) {
    console.log(`[getApiKey] Found in localStorage: ${storageKey.substring(0, 8)}...`);
    return storageKey.trim();
  }
  console.log('[getApiKey] Not found in localStorage');
  
  console.log(`[getApiKey] No API key found for ${provider}`);
  return null;
}

/**
 * Get API key from localStorage only (synchronous)
 */
export function getStoredApiKey(provider: 'openai' | 'anthropic'): string | null {
  return localStorage.getItem(`${provider}_api_key`);
}

export function setStoredApiKey(provider: 'openai' | 'anthropic', key: string) {
  localStorage.setItem(`${provider}_api_key`, key);
}

export function clearStoredApiKey(provider: 'openai' | 'anthropic') {
  localStorage.removeItem(`${provider}_api_key`);
}
