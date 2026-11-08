// The game runs from a copy on the device: after the first visit nothing is downloaded again
// (also offline, and as an installed Add to Home Screen app). Only the player stats go out.
// Each build stamps its own CACHE_VERSION into this file, so the browser's routine check of
// sw.js spots a new version, downloads it in the background and swaps it in (see index.html).
const CACHE = "silo-CACHE_VERSION";
const FILES = [
  "./",
  "./index.html",
  "./gl.js",
  "./audio.js",
  "./audio-unlock.js",
  "./stats.js",
  "./silo_gfx.wasm",
  "./manifest.webmanifest",
  "./icon-192.png",
  "./icon-512.png",
  "./apple-touch-icon.png",
];

self.addEventListener("install", (e) => {
  // cache: "reload" skips the browser's HTTP cache (GitHub Pages lets it keep files 10 minutes),
  // or a new version could be stored with the previous version's files.
  const fresh = FILES.map((f) => new Request(f, { cache: "reload" }));
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(fresh)).then(() => self.skipWaiting()));
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
  const req = e.request;
  // Only the game's own files; the player stats go straight to the network.
  if (req.method !== "GET" || new URL(req.url).origin !== self.location.origin) return;
  e.respondWith(caches.match(req, { ignoreSearch: true }).then((hit) => hit || fetch(req)));
});
