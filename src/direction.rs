#[derive(Debug, Clone, Copy)]
pub(crate) enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub(crate) const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    pub(crate) fn rotate(self, turns: usize) -> Direction {
        Direction::ALL[(self as usize + turns) % 4]
    }

    pub(crate) fn opposite(self) -> Direction {
        Direction::ALL[(self as usize + 2) % 4]
    }
}
