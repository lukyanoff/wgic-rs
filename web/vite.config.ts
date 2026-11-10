import http from 'node:http'
import https from 'node:https'
import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

// Generic dev proxy. The browser sends /__proxy/<rpc path> with an
// `x-target: http://host:port` header, and this middleware streams the request
// to that server and the response back. Any server works without CORS during
// development. It is dev-only; Vite binds to localhost by default.
const PROXY_PREFIX = '/__proxy'

function parseTarget(header: string | string[] | undefined): URL {
  const value = Array.isArray(header) ? header[0] : header
  if (!value) throw new Error('missing x-target header')
  const url = new URL(value)
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error(`unsupported x-target protocol: ${url.protocol}`)
  }
  return url
}

function connectProxy(): Plugin {
  return {
    name: 'connect-dev-proxy',
    configureServer(server) {
      server.middlewares.use(PROXY_PREFIX, (req, res) => {
        let target: URL
        try {
          target = parseTarget(req.headers['x-target'])
        } catch (e) {
          res.statusCode = 400
          res.end(String(e))
          return
        }
        const headers: http.OutgoingHttpHeaders = { ...req.headers, host: target.host }
        delete headers['x-target']
        delete headers.origin
        delete headers.referer
        const basePath = target.pathname.replace(/\/+$/, '')
        const upstream = (target.protocol === 'https:' ? https : http).request(
          {
            protocol: target.protocol,
            hostname: target.hostname,
            port: target.port,
            method: req.method,
            path: basePath + (req.url ?? '/'),
            headers,
          },
          (up) => {
            res.writeHead(up.statusCode ?? 502, up.headers)
            up.pipe(res)
          },
        )
        upstream.on('error', (err) => {
          // Connect-style error body, so the playground shows the real cause.
          if (!res.headersSent) {
            res.statusCode = 503
            res.setHeader('content-type', 'application/json')
          }
          res.end(
            JSON.stringify({
              code: 'unavailable',
              message: `dev proxy: ${target.origin}: ${err.message}`,
            }),
          )
        })
        req.pipe(upstream)
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), connectProxy()],
})
