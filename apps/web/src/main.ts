import { mount } from 'svelte';
import App from './App.svelte';
import '@xterm/xterm/css/xterm.css';
import './style.css';
const root = document.getElementById('app');
if (!root) throw new Error('Missing app root');
mount(App, { target: root });

if ('serviceWorker' in navigator && window.isSecureContext) {
  navigator.serviceWorker.register('/sw.js').catch(() => {});
}
