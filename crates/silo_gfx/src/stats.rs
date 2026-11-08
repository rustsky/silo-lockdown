//! Live player numbers, filled in by `web/stats.js` on the published web version. Elsewhere
//! (desktop, mobile builds, local or single-file web) there are none and the panel stays hidden.

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn silo_stats_online() -> i32;
    fn silo_stats_players() -> i32;
}

fn known(v: i32) -> Option<u32> {
    (v >= 0).then_some(v as u32)
}

/// Games open right now, once there is enough data for an estimate.
pub fn online() -> Option<u32> {
    #[cfg(target_arch = "wasm32")]
    {
        known(unsafe { silo_stats_online() })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        known(shot_value(0))
    }
}

/// Devices that have ever opened the game.
pub fn players() -> Option<u32> {
    #[cfg(target_arch = "wasm32")]
    {
        known(unsafe { silo_stats_players() })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        known(shot_value(1))
    }
}

/// Screenshots of the panel outside the web: SILO_SHOT_STATS=ONLINE,PLAYERS (-1 = unknown).
#[cfg(not(target_arch = "wasm32"))]
fn shot_value(i: usize) -> i32 {
    std::env::var("SILO_SHOT_STATS")
        .ok()
        .and_then(|v| v.split(',').nth(i)?.trim().parse().ok())
        .unwrap_or(-1)
}

/// 12345 -> "12,345".
pub fn grouped(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_and_unknown() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(known(-1), None);
        assert_eq!(known(3), Some(3));
    }
}
