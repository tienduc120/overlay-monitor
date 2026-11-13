//! Fixture.
#[derive(Clone, Copy)]
pub enum Light {
    Red,
    Green,
    Amber,
}

pub const fn go(light: Light) -> bool {
    match light {
        Light::Green => true,
        _ => false,
    }
}
