import React, { useRef, useEffect, useCallback } from 'react';
import { useStore } from '../store';

const WaveformVisualizer: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const { waveform, updateWaveform, isPlaying } = useStore();

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const { width, height } = canvas;
    const midY = height / 2;

    // Clear with gradient background
    const gradient = ctx.createLinearGradient(0, 0, 0, height);
    gradient.addColorStop(0, '#0d0d2b');
    gradient.addColorStop(0.5, '#121233');
    gradient.addColorStop(1, '#0d0d2b');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);

    // Draw center line
    ctx.strokeStyle = '#222255';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, midY);
    ctx.lineTo(width, midY);
    ctx.stroke();

    // Draw grid lines
    ctx.strokeStyle = '#1a1a40';
    ctx.lineWidth = 0.5;
    for (let i = 1; i < 4; i++) {
      const y = (height / 4) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
      ctx.stroke();
    }

    if (!waveform || waveform.length === 0) return;

    // Draw waveform
    const step = waveform.length / width;
    
    // Glow effect
    ctx.shadowColor = '#00ff88';
    ctx.shadowBlur = 8;
    
    // Main waveform line
    ctx.strokeStyle = '#00ff88';
    ctx.lineWidth = 2;
    ctx.beginPath();

    for (let x = 0; x < width; x++) {
      const idx = Math.floor(x * step);
      const sample = waveform[idx] || 0;
      const y = midY - sample * midY * 0.9;

      if (x === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    }
    ctx.stroke();

    // Draw filled area under waveform
    ctx.shadowBlur = 0;
    const fillGradient = ctx.createLinearGradient(0, 0, 0, height);
    fillGradient.addColorStop(0, 'rgba(0, 255, 136, 0.15)');
    fillGradient.addColorStop(0.5, 'rgba(0, 255, 136, 0.05)');
    fillGradient.addColorStop(1, 'rgba(0, 255, 136, 0.15)');
    ctx.fillStyle = fillGradient;
    ctx.beginPath();
    ctx.moveTo(0, midY);
    for (let x = 0; x < width; x++) {
      const idx = Math.floor(x * step);
      const sample = waveform[idx] || 0;
      const y = midY - sample * midY * 0.9;
      ctx.lineTo(x, y);
    }
    ctx.lineTo(width, midY);
    ctx.closePath();
    ctx.fill();

    // Draw a secondary lower-opacity waveform for depth
    ctx.strokeStyle = 'rgba(100, 200, 255, 0.3)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let x = 0; x < width; x++) {
      const idx = Math.floor(x * step);
      const sample = (waveform[idx] || 0) * 0.7;
      const y = midY - sample * midY * 0.9;
      if (x === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    }
    ctx.stroke();
  }, [waveform]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas) {
      const resize = () => {
        canvas.width = canvas.offsetWidth * window.devicePixelRatio;
        canvas.height = canvas.offsetHeight * window.devicePixelRatio;
        const ctx = canvas.getContext('2d');
        if (ctx) {
          ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
        }
        canvas.width = canvas.offsetWidth;
        canvas.height = canvas.offsetHeight;
      };
      resize();
      window.addEventListener('resize', resize);
      return () => window.removeEventListener('resize', resize);
    }
  }, []);

  useEffect(() => {
    let running = true;

    const animate = async () => {
      if (!running) return;
      await updateWaveform();
      draw();
      animationRef.current = requestAnimationFrame(animate);
    };

    animate();

    return () => {
      running = false;
      cancelAnimationFrame(animationRef.current);
    };
  }, [draw, updateWaveform]);

  return (
    <div className="waveform-container">
      <div className="waveform-label">
        <span className={`status-dot ${isPlaying ? 'active' : ''}`} />
        SCOPE
      </div>
      <canvas ref={canvasRef} className="waveform-canvas" />
    </div>
  );
};

export default WaveformVisualizer;
