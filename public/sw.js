/**
 * Shell-only service worker for the mobile PWA.
 *
 * Caches the static app shell so Sable opens from the Home Screen with no network. It
 * deliberately does NOT cache `/api/*`: those responses are authenticated and carry the full
 * financial position, and the app already keeps its last good payload in localStorage.
 * Keeping data out of the Cache API leaves exactly one place to clear.
 */
const CACHE = "sable-shell-v2";

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(["/", "/manifest.webmanifest"]))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

function cacheIfOk(request, response) {
  if (response.ok) {
    const copy = response.clone();
    void caches.open(CACHE).then((cache) => cache.put(request, copy));
  }
  return response;
}

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  // Never intercept the API: stale financial data must not be served behind the app's back,
  // and the 401 flow depends on reaching the server.
  if (url.origin !== self.location.origin || url.pathname.startsWith("/api/")) return;

  // The HTML document names its assets by content hash, so a cached copy goes stale the
  // moment the desktop rebuilds -- it would point at filenames the server no longer has.
  // Network-first keeps the app correct online and still opens offline.
  if (request.mode === "navigate") {
    event.respondWith(
      fetch(request)
        .then((response) => cacheIfOk(request, response))
        .catch(() => caches.match(request).then((cached) => cached ?? caches.match("/"))),
    );
    return;
  }

  // Only /assets/ is content-hashed and therefore immutable, so cache-first can never be wrong
  // there. The manifest and the icons keep stable names, so a cached copy would pin an
  // installed Home Screen app to the old artwork until CACHE is bumped -- they go network-first
  // like the document, falling back to cache so the offline shell still opens.
  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(
      caches.match(request).then(
        (cached) =>
          cached ??
          fetch(request)
            .then((response) => cacheIfOk(request, response))
            .catch(() => Response.error()),
      ),
    );
    return;
  }

  event.respondWith(
    fetch(request)
      .then((response) => cacheIfOk(request, response))
      .catch(() => caches.match(request).then((cached) => cached ?? Response.error())),
  );
});
