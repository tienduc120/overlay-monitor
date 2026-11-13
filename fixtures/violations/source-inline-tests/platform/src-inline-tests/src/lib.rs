//! Fixture.
pub const fn one() -> u8 {
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_is_one() {
        assert_eq!(super::one(), 1);
    }
}
