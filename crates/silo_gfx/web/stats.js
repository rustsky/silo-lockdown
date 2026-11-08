"use strict";
// Live player numbers for the title screen, without a server of our own: two public counters on
// abacus.jasoncameron.dev (free, no account, CORS open). Nothing personal is sent or stored.
//
//  - "players": +1 the first time a device opens the game (remembered in localStorage).
//  - "beats":   +1 every 30 s from every open, visible game. How fast it rises tells how many
//               games are open right now: N players add N per 30 s.
//
// The game reads the results through silo_stats_online / silo_stats_players (-1 = unknown). Only
// runs on the real site over HTTPS, so local builds and the single-file version don't count.
(function () {
    var API = "https://abacus.jasoncameron.dev";
    var NS = "silo-lockdown";
    var BEAT_MS = 30000;
    var WINDOW_MS = 125000; // estimate over up to four beats to smooth out timing jitter

    var online = -1;
    var players = -1;
    var samples = []; // [time ms, beats counter value]

    var host = location.hostname;
    var enabled = location.protocol === "https:" && host !== "localhost" && host !== "127.0.0.1";

    function call(op, key) {
        return fetch(API + "/" + op + "/" + NS + "/" + key, { cache: "no-store" })
            .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
            .then(function (j) { return typeof j.value === "number" ? j.value : Promise.reject("bad"); });
    }

    function storage(key, value) {
        try {
            if (value === undefined) return localStorage.getItem(key);
            localStorage.setItem(key, value);
        } catch (e) { /* private mode or blocked storage: count again, no harm */ }
        return null;
    }

    function countPlayer() {
        var counted = storage("silo-stats-counted") === "1";
        call(counted ? "get" : "hit", "players").then(function (v) {
            players = v;
            if (!counted) storage("silo-stats-counted", "1");
        }).catch(function () {});
    }

    function beat() {
        if (document.visibilityState !== "visible") return;
        call("hit", "beats").then(function (v) {
            var now = Date.now();
            samples.push([now, v]);
            samples = samples.filter(function (s) { return now - s[0] <= WINDOW_MS; });
            var first = samples[0];
            if (samples.length < 2 || now - first[0] < BEAT_MS * 0.8) return;
            var rate = (v - first[1]) / ((now - first[0]) / BEAT_MS);
            online = Math.max(1, Math.round(rate)); // we are playing, so at least one
        }).catch(function () {});
    }

    if (enabled) {
        countPlayer();
        beat();
        setInterval(beat, BEAT_MS);
        // Back from the background: the old samples have a gap, but the rate stays right.
        document.addEventListener("visibilitychange", function () {
            if (document.visibilityState === "visible") samples = samples.slice(-1);
        });
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.silo_stats_online = function () { return online; };
            importObject.env.silo_stats_players = function () { return players; };
        },
        name: "silo_stats",
        version: 1,
    });
})();
