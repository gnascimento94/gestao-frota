import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './estilo/tokens.css'
import './estilo/base.css'
import './estilo/ecras.css'
import './estilo/lancar.css'
import App from './App.tsx'
import { marcarSemMica } from './util/mica'

void marcarSemMica()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
