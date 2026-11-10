import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App.tsx'
import { loadMethods } from './schema'

const root = createRoot(document.getElementById('root')!)

loadMethods().then(
  (methods) =>
    root.render(
      <StrictMode>
        <App methods={methods} />
      </StrictMode>,
    ),
  (err: unknown) => root.render(<pre className="error-box">{String(err)}</pre>),
)
