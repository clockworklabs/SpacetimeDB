

pub struct HexCoordinates {
    pub x: i32,
    pub z: i32,
}

#[derive(PartialEq, PartialOrd, Eq, Clone, Copy)]
pub enum HexDirection {
    NE = 0, // Flat up right
    ENE,    // Pointy up right
    E,      // Flat right
    ESE,    // Pointy down right
    SE,     // Flat down right
    S,      // Pointy down
    SW,     // Flat down left
    WSW,    // Pointy down left
    W,      // Flat left
    WNW,    // Pointy up left
    NW,     // Flat up left
    N,      // Pointy up
}

impl From<i32> for HexDirection {
    fn from(int: i32) -> Self {
        match int {
            0 => Self::NE,  // Flat up right
            1 => Self::ENE, // Pointy up right
            2 => Self::E,   // Flat right
            3 => Self::ESE, // Pointy down right
            4 => Self::SE,  // Flat down right
            5 => Self::S,   // Pointy down
            6 => Self::SW,  // Flat down left
            7 => Self::WSW, // Pointy down left
            8 => Self::W,   // Flat left
            9 => Self::WNW, // Pointy up left
            10 => Self::NW, // Flat up left
            11 => Self::N,  // Pointy up
            _ => panic!("Invalid HexDirection {}", int),
        }
    }
}
