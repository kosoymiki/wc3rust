//! Race music playlists: a deterministic, hash-ordered rotation.
//!
//! Retail WC3 plays a race-specific set of themes (`Human1..3`, `Orc1..3`,
//! `NightElf1..3`, `Undead1..3` — the `musictheme` rows in
//! `UI/TriggerData.txt`) and cycles through it for the length of the match.
//! Selection order must be **stable across runs** (replays, tests, device
//! verification logs) and must not consume the simulation's RNG, so the order
//! is derived from a hash of `(race, track)` rather than a random shuffle.
//!
//! The playlist is data-only: playback, cross-fades and the PCM budget live in
//! [`crate::game_audio::GameAudio`], which owns the mixer.

use crate::events::{MusicState, Race};

/// FNV-1a 64: the project's stable hash for deterministic ordering.
pub fn fnv1a(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Order `names` by `fnv1a(seed + name)`.
///
/// The seed is the race id, so each race gets its own stable order while the
/// order stays identical for every run of the same race.
pub fn hash_order<'a>(seed: &str, names: &[&'a str]) -> Vec<&'a str> {
    let mut ordered: Vec<(u64, &'a str)> = names
        .iter()
        .map(|name| (fnv1a(&format!("{seed}/{name}")), *name))
        .collect();
    ordered.sort_by_key(|(hash, _)| *hash);
    ordered.into_iter().map(|(_, name)| name).collect()
}

/// One race's looping theme rotation for a game state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicPlaylist {
    race: Race,
    state: MusicState,
    tracks: Vec<&'static str>,
    cursor: usize,
}

impl MusicPlaylist {
    /// Build the playlist for a race/state from the shipped theme rows.
    pub fn build(race: Race, state: MusicState) -> Self {
        let tracks = hash_order(race.id(), race.theme_names(state));
        Self {
            race,
            state,
            tracks,
            cursor: 0,
        }
    }

    pub fn race(&self) -> Race {
        self.race
    }

    pub fn state(&self) -> MusicState {
        self.state
    }

    pub fn tracks(&self) -> &[&'static str] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Track the playlist is currently on.
    pub fn current(&self) -> Option<&'static str> {
        self.tracks.get(self.cursor).copied()
    }

    /// Advance to the next track (wrapping) and return it.
    pub fn next(&mut self) -> Option<&'static str> {
        if self.tracks.is_empty() {
            return None;
        }
        self.cursor = (self.cursor + 1) % self.tracks.len();
        self.current()
    }

    /// Restart the rotation at its first track.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Position the rotation on track `index` (wrapping, no-op when empty).
    pub fn set_cursor(&mut self, index: usize) {
        if !self.tracks.is_empty() {
            self.cursor = index % self.tracks.len();
        }
    }

    /// Current position in the rotation.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_order_is_stable_and_permuted() {
        let names = ["Human1", "Human2", "Human3"];
        let first = hash_order("human", &names);
        let second = hash_order("human", &names);
        assert_eq!(first, second, "same seed => same order");
        let mut sorted = first.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec!["Human1", "Human2", "Human3"]);
        // A different seed must be able to produce a different rotation.
        let human_orders: Vec<_> = Race::ALL
            .iter()
            .map(|race| hash_order(race.id(), &names))
            .collect();
        assert!(
            human_orders.windows(2).any(|pair| pair[0] != pair[1]),
            "race seeds must not all collapse to one order"
        );
    }

    #[test]
    fn playlist_wraps_through_every_track() {
        let mut playlist = MusicPlaylist::build(Race::Orc, MusicState::Playing);
        assert_eq!(playlist.len(), 3);
        let first = playlist.current().expect("first track");
        let mut seen = vec![first];
        for _ in 0..2 {
            seen.push(playlist.next().expect("next track"));
        }
        assert_eq!(playlist.next(), Some(first), "wraps to the first track");
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 3, "all three race themes are in the rotation");
    }
}
