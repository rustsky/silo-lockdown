// Offline support: cache the whole game on the device on first visit, then serve it from the
// cache, so the installed app (Add to Home Screen) runs without a network.
// CACHE_VERSION is replaced at build time with a hash of the build.
const CACHE = "silo-CACHE_VERSION";
const FILES = [
  "./",
  "./index.html",
  "./gl.js",
  "./audio.js",
  "./audio-unlock.js",
  "./silo_gfx.wasm",
  "./manifest.webmanifest",
  "./icon-192.png",
  "./icon-512.png",
  "./apple-touch-icon.png",
];

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (e) => {
  if (e.request.method !== "GET") return;
  e.respondWith(
    caches.match(e.request, { ignoreSearch: true }).then((hit) => hit || fetch(e.request))
  );
});
