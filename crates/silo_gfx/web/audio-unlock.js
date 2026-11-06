// iOS/Safari audio unlock for quad-snd (audio.js).
// quad-snd resumes its AudioContext on the first touchstart and then stops listening, but
// iOS only lets audio start from touchend/click, so on iPhone the game stays silent. Here we
// keep trying on every real user gesture until the context is actually running, ask iOS to
// treat us as media playback (so the ring/silent switch doesn't mute the game), and resume
// after the app comes back from the background.
(function () {
  try {
    if (navigator.audioSession) navigator.audioSession.type = "playback"; // iOS 17+
  } catch (e) {}

  // Older iOS: Web Audio follows the silent switch unless an HTML <audio> element is
  // playing too. Loop a short silent WAV (built here, no file needed) once a gesture allows.
  var silent = null;
  function silentWavUrl() {
    var rate = 8000, n = rate / 2, buf = new ArrayBuffer(44 + n), v = new DataView(buf);
    function s(o, t) { for (var i = 0; i < t.length; i++) v.setUint8(o + i, t.charCodeAt(i)); }
    s(0, "RIFF"); v.setUint32(4, 36 + n, true); s(8, "WAVE"); s(12, "fmt ");
    v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
    v.setUint32(24, rate, true); v.setUint32(28, rate, true); v.setUint16(32, 1, true);
    v.setUint16(34, 8, true); s(36, "data"); v.setUint32(40, n, true);
    for (var i = 0; i < n; i++) v.setUint8(44 + i, 128); // 8-bit silence
    return URL.createObjectURL(new Blob([buf], { type: "audio/wav" }));
  }
  function playSilent() {
    try {
      if (!silent) {
        silent = document.createElement("audio");
        silent.setAttribute("playsinline", "");
        silent.setAttribute("x-webkit-airplay", "deny");
        silent.preload = "auto";
        silent.loop = true;
        silent.src = silentWavUrl();
      }
      if (silent.paused) {
        var p = silent.play();
        if (p && p.catch) p.catch(function () {});
      }
    } catch (e) {}
  }

  function unlock() {
    playSilent();
    if (typeof audio_context === "undefined" || !audio_context) return;
    if (audio_context.state === "running") return;
    var p = audio_context.resume();
    if (p && p.catch) p.catch(function () {});
    // Older Safari also needs a sound started inside the gesture.
    try {
      var src = audio_context.createBufferSource();
      src.buffer = audio_context.createBuffer(1, 1, 22050);
      src.connect(audio_context.destination);
      src.start(0);
    } catch (e) {}
  }

  ["touchstart", "touchend", "pointerup", "click", "keydown"].forEach(function (ev) {
    document.addEventListener(ev, unlock, true);
  });
  document.addEventListener("visibilitychange", function () {
    if (!document.hidden) unlock();
  });
})();
