import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><App/></React.StrictMode>);
if (import.meta.env.VITE_NATIVE_SMOKE === '1') {
  void import('../features/repository/nativeSmoke').then(module => module.runNativeSmoke());
}
