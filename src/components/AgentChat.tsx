import React, { useState, useRef, useEffect } from 'react';
import { useStore, AgentMessage } from '../store';
import { FaTimes, FaPaperPlane, FaCode, FaMagic, FaTrash, FaPlus, FaCog } from 'react-icons/fa';
import { reactiveAgentProcess, setStoredApiKey, getApiKey, AVAILABLE_MODELS, LLMProvider, ModelId } from '../llm';

const AgentChat: React.FC = () => {
  const {
    showAgentChat,
    toggleAgentChat,
    agentMessages,
    addAgentMessage,
    clearAgentMessages,
    buffers,
    activeBufferId,
    updateBufferCode,
    agentProvider,
    agentModel,
    setAgentProvider,
    setAgentModel,
  } = useStore();

  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [apiKeys, setApiKeys] = useState({
    openai: '',
    anthropic: '',
  });
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Load API keys from all sources on mount
  useEffect(() => {
    const loadApiKeys = async () => {
      const openaiKey = await getApiKey('openai');
      const anthropicKey = await getApiKey('anthropic');
      setApiKeys({
        openai: openaiKey || '',
        anthropic: anthropicKey || '',
      });
    };
    loadApiKeys();
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [agentMessages]);

  useEffect(() => {
    if (showAgentChat && inputRef.current) {
      inputRef.current.focus();
    }
  }, [showAgentChat]);

  if (!showAgentChat) return null;

  const activeBuffer = buffers.find(b => b.id === activeBufferId);
  const currentCode = activeBuffer?.code || '';

  const handleSend = async () => {
    const trimmed = input.trim();
    if (!trimmed || isThinking) return;

    addAgentMessage({ role: 'user', content: trimmed });
    setInput('');
    setIsThinking(true);

    try {
      // Use reactive LLM agent
      let apiKey: string | undefined = undefined;
      if (agentProvider === 'openai') {
        apiKey = apiKeys.openai || undefined;
      } else if (agentProvider === 'anthropic') {
        apiKey = apiKeys.anthropic || undefined;
      }

      console.log('[AgentChat] Sending message:', {
        provider: agentProvider,
        model: agentModel,
        hasApiKey: !!apiKey,
        apiKeyLength: apiKey?.length || 0,
      });

      const response = await reactiveAgentProcess(
        {
          provider: agentProvider,
          model: agentModel,
          apiKey,
          maxReflections: 2,
        },
        {
          currentCode,
          conversationHistory: agentMessages,
          userMessage: trimmed,
        }
      );

      addAgentMessage(response);
    } catch (error: any) {
      console.error('[AgentChat] Agent error:', error);
      
      // Provide helpful error messages based on error type
      let errorMessage = 'Sorry, I encountered an error.';
      
      if (error?.message?.includes('API key')) {
        errorMessage = '❌ Invalid API key. Please check your API key in Settings (⚙️) or verify it at platform.openai.com or console.anthropic.com.';
      } else if (error?.message?.includes('rate_limit') || error?.message?.includes('quota')) {
        errorMessage = '⚠️ API rate limit or quota exceeded. Please try again later or switch to Local mode.';
      } else if (error?.message?.includes('model') || error?.message?.includes('not found')) {
        errorMessage = `❌ Model "${agentModel}" not found or not available. Try switching to a different model.`;
      } else if (error?.status === 400) {
        errorMessage = `❌ API Error: ${error.message || 'Bad request'}. The model may not support the requested parameters.`;
      } else if (error?.status === 401 || error?.status === 403) {
        errorMessage = '🔒 Authentication failed. Please check your API key in Settings (⚙️).';
      } else if (agentProvider !== 'local') {
        const hasKey = (agentProvider === 'openai' && apiKeys.openai) || (agentProvider === 'anthropic' && apiKeys.anthropic);
        if (!hasKey) {
          errorMessage = '⚠️ No API key configured. Please add your API key in Settings (⚙️) or switch to Local mode.';
        }
      }
      
      addAgentMessage({
        role: 'assistant',
        content: errorMessage,
      });
    } finally {
      setIsThinking(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleInsertCode = (code: string) => {
    const newCode = currentCode.endsWith('\n')
      ? currentCode + code + '\n'
      : currentCode + '\n' + code + '\n';
    updateBufferCode(activeBufferId, newCode);
    addAgentMessage({
      role: 'assistant',
      content: '✅ Code inserted into the current buffer.',
    });
  };

  const handleReplaceCode = (code: string) => {
    updateBufferCode(activeBufferId, code);
    addAgentMessage({
      role: 'assistant',
      content: '✅ Buffer code replaced with the refactored version.',
    });
  };

  const quickActions = [
    {
      label: 'Generate a beat',
      icon: <FaCode />,
      prompt: 'Generate a cool drum beat pattern using live_loop with kick, snare, and hihat samples.',
    },
    {
      label: 'Refactor my code',
      icon: <FaMagic />,
      prompt: 'Refactor the current code in my buffer. Make it cleaner, more idiomatic Sonic Pi, and better structured.',
    },
    {
      label: 'Add effects',
      icon: <FaMagic />,
      prompt: 'Suggest some effects I can add to improve my current code. Provide code with with_fx blocks.',
    },
    {
      label: 'Explain my code',
      icon: <FaCode />,
      prompt: 'Explain what the current code in my buffer does, line by line.',
    },
  ];

  return (
    <div className="side-panel agent-chat-panel">
      <div className="panel-header">
        <h3>
          <span className="agent-icon">🤖</span> Agent
        </h3>
        <div className="agent-header-actions">
          <button 
            className="close-btn" 
            onClick={() => setShowSettings(true)} 
            title="Settings & API Keys"
          >
            <FaCog />
          </button>
          <button className="close-btn" onClick={clearAgentMessages} title="Clear chat">
            <FaTrash />
          </button>
          <button className="close-btn" onClick={toggleAgentChat}>
            <FaTimes />
          </button>
        </div>
      </div>

      {/* Model Selector Bar */}
      <div className="agent-model-selector">
        <select
          value={agentProvider}
          onChange={(e) => {
            const newProvider = e.target.value as LLMProvider;
            setAgentProvider(newProvider);
            // Auto-select first model for new provider
            const firstModel = AVAILABLE_MODELS[newProvider][0].id;
            setAgentModel(firstModel);
          }}
          className="agent-select"
        >
          <option value="local">Local (Offline)</option>
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
        </select>
        <select
          value={agentModel}
          onChange={(e) => setAgentModel(e.target.value as ModelId)}
          className="agent-select"
        >
          {AVAILABLE_MODELS[agentProvider].map((model) => (
            <option key={model.id} value={model.id}>
              {model.name}
            </option>
          ))}
        </select>
      </div>

      {/* Settings Modal */}
      {showSettings && (
        <SettingsModal
          apiKeys={apiKeys}
          onSave={(keys) => {
            setApiKeys(keys);
            if (keys.openai) setStoredApiKey('openai', keys.openai);
            if (keys.anthropic) setStoredApiKey('anthropic', keys.anthropic);
            setShowSettings(false);
          }}
          onClose={() => setShowSettings(false)}
        />
      )}

      <div className="agent-messages">
        {agentMessages.length === 0 && (
          <div className="agent-welcome">
            <div className="agent-welcome-icon">🎵</div>
            <p className="agent-welcome-title">PiBeat Agent</p>
            <p className="agent-welcome-desc">
              I know Sonic Pi inside and out. Ask me to generate beats, 
              refactor your code, explain syntax, or suggest improvements.
            </p>
            <div className="agent-quick-actions">
              {quickActions.map((action, i) => (
                <button
                  key={i}
                  className="agent-quick-btn"
                  onClick={() => {
                    setInput(action.prompt);
                    inputRef.current?.focus();
                  }}
                >
                  {action.icon}
                  <span>{action.label}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {agentMessages.map((msg, i) => (
          <div key={i} className={`agent-msg agent-msg-${msg.role}`}>
            <div className="agent-msg-label">
              {msg.role === 'user' ? 'You' : 'Agent'}
            </div>
            <div className="agent-msg-content">
              <MessageContent
                message={msg}
                onInsert={handleInsertCode}
                onReplace={handleReplaceCode}
              />
            </div>
          </div>
        ))}

        {isThinking && (
          <div className="agent-msg agent-msg-assistant">
            <div className="agent-msg-label">Agent</div>
            <div className="agent-msg-content">
              <div className="agent-thinking">
                <span className="dot" />
                <span className="dot" />
                <span className="dot" />
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      <div className="agent-input-area">
        <div className="agent-context-badge" title="The agent can see your current buffer code">
          📋 Buffer {activeBufferId}
        </div>
        <div className="agent-input-row">
          <textarea
            ref={inputRef}
            className="agent-input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask about Sonic Pi, request code, or say 'refactor'..."
            rows={2}
            disabled={isThinking}
          />
          <button
            className="agent-send-btn"
            onClick={handleSend}
            disabled={!input.trim() || isThinking}
            title="Send (Enter)"
          >
            <FaPaperPlane />
          </button>
        </div>
      </div>
    </div>
  );
};

/** Renders a message, extracting code blocks and adding Insert/Replace buttons */
const MessageContent: React.FC<{
  message: AgentMessage;
  onInsert: (code: string) => void;
  onReplace: (code: string) => void;
}> = ({ message, onInsert, onReplace }) => {
  if (message.role === 'user') {
    return <span>{message.content}</span>;
  }

  // Parse code blocks from assistant messages
  const parts = message.content.split(/(```[\s\S]*?```)/g);

  return (
    <>
      {parts.map((part, i) => {
        const codeMatch = part.match(/^```(?:\w*)\n?([\s\S]*?)```$/);
        if (codeMatch) {
          const code = codeMatch[1].trim();
          return (
            <div key={i} className="agent-code-block">
              <pre><code>{code}</code></pre>
              <div className="agent-code-actions">
                <button
                  className="agent-code-btn agent-insert-btn"
                  onClick={() => onInsert(code)}
                  title="Append this code to your current buffer"
                >
                  <FaPlus /> Insert
                </button>
                <button
                  className="agent-code-btn agent-replace-btn"
                  onClick={() => onReplace(code)}
                  title="Replace your entire buffer with this code"
                >
                  <FaMagic /> Replace
                </button>
              </div>
            </div>
          );
        }
        // Render text parts — convert inline `code` 
        const inlineParts = part.split(/(`[^`]+`)/g);
        return (
          <span key={i}>
            {inlineParts.map((ip, j) => {
              if (ip.startsWith('`') && ip.endsWith('`')) {
                return <code key={j} className="agent-inline-code">{ip.slice(1, -1)}</code>;
              }
              return <span key={j}>{ip}</span>;
            })}
          </span>
        );
      })}
    </>
  );
};

/** Settings modal for API keys */
const SettingsModal: React.FC<{
  apiKeys: { openai: string; anthropic: string };
  onSave: (keys: { openai: string; anthropic: string }) => void;
  onClose: () => void;
}> = ({ apiKeys, onSave, onClose }) => {
  const [openaiKey, setOpenaiKey] = useState(apiKeys.openai);
  const [anthropicKey, setAnthropicKey] = useState(apiKeys.anthropic);

  return (
    <div className="agent-settings-overlay" onClick={onClose}>
      <div className="agent-settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="agent-settings-header">
          <h3>⚙️ LLM Settings</h3>
          <button className="close-btn" onClick={onClose}>
            <FaTimes />
          </button>
        </div>
        <div className="agent-settings-content">
          <div className="settings-section">
            <label>OpenAI API Key</label>
            <input
              type="password"
              value={openaiKey}
              onChange={(e) => setOpenaiKey(e.target.value)}
              placeholder="sk-..."
              className="agent-settings-input"
            />
            <p className="settings-hint">
              Get your key from{' '}
              <a href="https://platform.openai.com/api-keys" target="_blank" rel="noreferrer">
                platform.openai.com
              </a>
            </p>
          </div>
          <div className="settings-section">
            <label>Anthropic API Key</label>
            <input
              type="password"
              value={anthropicKey}
              onChange={(e) => setAnthropicKey(e.target.value)}
              placeholder="sk-ant-..."
              className="agent-settings-input"
            />
            <p className="settings-hint">
              Get your key from{' '}
              <a href="https://console.anthropic.com/settings/keys" target="_blank" rel="noreferrer">
                console.anthropic.com
              </a>
            </p>
          </div>
          <div className="settings-section">
            <p className="settings-note">
              ℹ️ <strong>Priority:</strong> 1) System environment variables (OPENAI_API_KEY, ANTHROPIC_API_KEY), 2) .env file, 3) localStorage (below).
            </p>
            <p className="settings-note">
              💡 If you set system env vars, they will override these values. <strong>Local mode</strong> works offline with no API key required.
            </p>
          </div>
        </div>
        <div className="agent-settings-footer">
          <button className="agent-settings-btn cancel" onClick={onClose}>
            Cancel
          </button>
          <button
            className="agent-settings-btn save"
            onClick={() => onSave({ openai: openaiKey, anthropic: anthropicKey })}
          >
            Save Keys
          </button>
        </div>
      </div>
    </div>
  );
};

export default AgentChat;
