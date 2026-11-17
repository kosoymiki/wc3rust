//! Core deterministic types and fixed-point math for wc3rust.
//!
//! Simulation code uses `Fixed` (Q16.16) instead of raw `f32`; render code may use
//! `glam` floats. The conversion boundary is explicit (`Fixed::to_f32`).

use std::fmt;

pub use glam::{Vec2, Vec3};

/// Player slot in a match. 0..=23 (WC3 supports up to 24 slots).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(u8);

impl PlayerId {
    pub const MAX: u8 = 23;

    pub const fn new(raw: u8) -> Result<Self, CoreError> {
        if raw <= Self::MAX {
            Ok(Self(raw))
        } else {
            Err(CoreError::InvalidPlayer(raw))
        }
    }

    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// Stable unit handle. Generation is supplied by the ECS/arena layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId {
    pub index: u32,
    pub generation: u32,
}

/// Simulation tick, monotonic from match start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tick(pub u64);

impl Tick {
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Fixed-point number with 16 fractional bits (Q16.16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(i32);

impl Fixed {
    pub const FRAC_BITS: u32 = 16;
    pub const ONE: Self = Self(1 << Self::FRAC_BITS);
    pub const ZERO: Self = Self(0);

    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> i32 {
        self.0
    }

    pub const fn from_int(v: i32) -> Self {
        Self(v << Self::FRAC_BITS)
    }

    pub fn from_f32(v: f32) -> Self {
        Self((v * (1 << Self::FRAC_BITS) as f32).round() as i32)
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / (1 << Self::FRAC_BITS) as f32
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }

    /// Fixed multiply with 64-bit intermediate; saturates on overflow.
    pub fn saturating_mul(self, rhs: Self) -> Self {
        let v = (self.0 as i64 * rhs.0 as i64) >> Self::FRAC_BITS;
        Self(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }

    pub fn saturating_div(self, rhs: Self) -> Option<Self> {
        if rhs.0 == 0 {
            return None;
        }
        let v = ((self.0 as i64) << Self::FRAC_BITS) / rhs.0 as i64;
        Some(Self(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32))
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.4}", self.to_f32())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    InvalidPlayer(u8),
    DivideByZero,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPlayer(id) => write!(f, "invalid player id {id}"),
            Self::DivideByZero => write!(f, "fixed-point division by zero"),
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_one_roundtrips() {
        assert_eq!(Fixed::from_int(1), Fixed::ONE);
        assert_eq!(Fixed::from_f32(0.5).to_f32(), 0.5);
        assert_eq!(Fixed::from_int(-3).to_f32(), -3.0);
    }

    #[test]
    fn fixed_mul_and_div() {
        let two = Fixed::from_int(2);
        let three = Fixed::from_int(3);
        assert_eq!(two.saturating_mul(three), Fixed::from_int(6));
        assert_eq!(Fixed::from_int(6).saturating_div(two), Some(three));
        assert_eq!(two.saturating_div(Fixed::ZERO), None);
    }

    #[test]
    fn player_bounds() {
        assert!(PlayerId::new(0).is_ok());
        assert!(PlayerId::new(PlayerId::MAX).is_ok());
        assert!(PlayerId::new(PlayerId::MAX + 1).is_err());
    }

    #[test]
    fn tick_is_monotonic() {
        assert_eq!(Tick(7).next(), Tick(8));
    }
}
