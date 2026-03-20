import React from 'react';
import ReactDOM from 'react-dom/client';
import BandVisualizerWindow from './components/BandVisualizerWindow';
import './App.css';

ReactDOM.createRoot(document.getElementById('band-root') as HTMLElement).render(
  <React.StrictMode>
    <BandVisualizerWindow />
  </React.StrictMode>,
);
