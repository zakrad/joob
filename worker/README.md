# joob-drive Worker

A tiny Cloudflare Worker that proxies Google Drive and Google OAuth requests
for the joob client. Use it when Google's edge IPs are TCP-blocked by your ISP
(e.g. inside Iran) and the client cannot reach `googleapis.com` directly.

The Worker is stateless: it forwards requests verbatim to Google and returns
the response unchanged. No tokens or keys are stored.

## Routes

| Client path                  | Upstream                                     |
| ---------------------------- | -------------------------------------------- |
| `/drive/v3/...`              | `https://www.googleapis.com/drive/v3/...`    |
| `/upload/drive/v3/...`       | `https://www.googleapis.com/upload/drive/v3/...` |
| `/token`                     | `https://oauth2.googleapis.com/token`        |
| `/device/code`               | `https://oauth2.googleapis.com/device/code`  |
| `/` or `/health`             | `{ "ok": true, "service": "joob-drive" }`    |

## Deploy

```bash
npm install -g wrangler
cd worker
wrangler login          # opens a browser window
wrangler deploy
```

Deploy prints a URL like `https://joob-drive.<your-subdomain>.workers.dev`.
Pass this URL to `joob-server setup --drive-frontend-url <url>` to embed it in
generated client profiles.

## Optional shared-secret auth

By default the Worker is open (anyone can use it). To lock it down:

```bash
wrangler secret put WORKER_AUTH_TOKEN
# paste a long random string
```

Then deploy clients with `--drive-frontend-auth <same string>`.
Requests without the matching `X-Joob-Auth` header get 401.

## Verify

```bash
curl https://joob-drive.<your-subdomain>.workers.dev/health
# {"ok":true,"service":"joob-drive"}

curl https://joob-drive.<your-subdomain>.workers.dev/drive/v3/about
# {"error":{"code":401,"message":"Login Required",...}}
# ↑ Google's own 401 — proves the proxy reaches Google.
```

## Free tier

Cloudflare Workers Free Plan: 100k requests/day, 10 ms CPU per request. joob's
traffic pattern (one request per chunk upload/download, multipart streamed) is
well under that. CPU time is negligible because the Worker just pipes streams.
