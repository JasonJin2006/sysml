import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';
import App from './App';

const host = document.getElementById('root');
if (!host) throw new Error('index.html has no #root');

createRoot(host).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
