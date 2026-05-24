/**
 * joob-drive — Cloudflare Worker frontend for Google APIs.
 *
 * Path-based router that forwards requests to the appropriate Google upstream.
 * The joob client uses this Worker when Google's edge IPs are TCP-blocked
 * by the client's ISP.
 *
 * Routes:
 *   /drive/v3/...           → https://www.googleapis.com/drive/v3/...
 *   /upload/drive/v3/...    → https://www.googleapis.com/upload/drive/v3/...
 *   /token                  → https://oauth2.googleapis.com/token
 *   /device/code            → https://oauth2.googleapis.com/device/code
 *
 * Auth: If the secret `WORKER_AUTH_TOKEN` is set, requests must include a
 * matching `X-Joob-Auth` header. Otherwise the Worker is open.
 */

const ROUTES = [
  { prefix: "/drive/v3/",        upstream: "https://www.googleapis.com" },
  { prefix: "/upload/drive/v3/", upstream: "https://www.googleapis.com" },
  { prefix: "/token",            upstream: "https://oauth2.googleapis.com" },
  { prefix: "/device/code",      upstream: "https://oauth2.googleapis.com" },
];

export default {
  async fetch(request, env) {
    // Optional shared-secret auth
    if (env.WORKER_AUTH_TOKEN) {
      const provided = request.headers.get("X-Joob-Auth");
      if (provided !== env.WORKER_AUTH_TOKEN) {
        return new Response("unauthorized", { status: 401 });
      }
    }

    const url = new URL(request.url);

    // Health check / discovery
    if (url.pathname === "/" || url.pathname === "/health") {
      return new Response(JSON.stringify({ ok: true, service: "joob-drive" }), {
        headers: { "content-type": "application/json" },
      });
    }

    // Match route by prefix
    const route = ROUTES.find((r) => url.pathname.startsWith(r.prefix));
    if (!route) {
      return new Response("not found", { status: 404 });
    }

    // Build upstream URL: keep path + query verbatim, swap origin.
    const upstreamUrl = route.upstream + url.pathname + url.search;

    // Forward headers, stripping anything that would confuse the upstream.
    const fwdHeaders = new Headers(request.headers);
    fwdHeaders.delete("host");
    fwdHeaders.delete("x-joob-auth");
    fwdHeaders.delete("cf-connecting-ip");
    fwdHeaders.delete("cf-ipcountry");
    fwdHeaders.delete("cf-ray");
    fwdHeaders.delete("cf-visitor");
    fwdHeaders.delete("x-forwarded-for");
    fwdHeaders.delete("x-forwarded-proto");
    fwdHeaders.delete("x-real-ip");

    const upstreamReq = new Request(upstreamUrl, {
      method: request.method,
      headers: fwdHeaders,
      body:
        request.method === "GET" || request.method === "HEAD"
          ? undefined
          : request.body,
      redirect: "manual",
    });

    const upstreamResp = await fetch(upstreamReq);

    // Return response body streamed, headers passed through.
    const respHeaders = new Headers(upstreamResp.headers);
    // Strip hop-by-hop headers Cloudflare may otherwise re-add weirdly.
    respHeaders.delete("transfer-encoding");
    respHeaders.delete("connection");

    return new Response(upstreamResp.body, {
      status: upstreamResp.status,
      statusText: upstreamResp.statusText,
      headers: respHeaders,
    });
  },
};
