use image::{RgbaImage, imageops};
use rand::prelude::*;
use serde::Deserialize;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    env,
    error::Error,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

type SocketId = usize;
type Mask = u128;
type HeapEntry = (Reverse<u32>, u32, usize);

const DEFAULT_WIDTH: usize = 100;
const DEFAULT_HEIGHT: usize = 100;
const MAX_TILES: usize = Mask::BITS as usize;

#[derive(Debug, Clone, Copy)]
enum Direction {
    North,
    East,
    South,
    West,
}

#[derive(Debug, Deserialize)]
struct Socket(String);

#[derive(Debug, Deserialize)]
struct Pair {
    a: Socket,
    b: Socket,
}

#[derive(Debug, Deserialize)]
struct Adjacencies {
    north: Socket,
    east: Socket,
    south: Socket,
    west: Socket,
}

#[derive(Debug, Deserialize)]
struct OriginalTile {
    name: String,
    #[serde(default)]
    rotate: bool,
    adjacencies: Adjacencies,
}

#[derive(Debug, Deserialize)]
struct Config {
    format: String,
    #[serde(default)]
    pairs: Vec<Pair>,
    tiles: Vec<OriginalTile>,
}

#[derive(Debug)]
struct Tile {
    name: String,
    rotation: usize, // number of 90° clockwise turns
}

#[derive(Debug)]
struct Tileset {
    format: String,
    tiles: Vec<Tile>,
    allowed: [Vec<Mask>; 4],
}

#[derive(Debug, Clone)]
struct Cell {
    possible: Mask,
}

#[derive(Debug)]
struct Grid {
    width: usize,
    height: usize,
    cells: Vec<Cell>, // flat 2D grid
    heap: BinaryHeap<HeapEntry>,
    noise: Vec<u32>,
}

impl Direction {
    const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    fn rotate(self, turns: usize) -> Direction {
        Direction::ALL[(self as usize + turns) % 4]
    }

    fn opposite(self) -> Direction {
        Direction::ALL[(self as usize + 2) % 4]
    }
}

impl Tileset {
    fn from_config(config: Config) -> Result<Self, String> {
        fn intern(ids: &mut HashMap<String, SocketId>, socket: Socket) -> SocketId {
            let next = ids.len();
            *ids.entry(socket.0).or_insert(next)
        }

        let mut socket_ids: HashMap<String, SocketId> = HashMap::new();
        let mut tiles: Vec<Tile> = Vec::new();
        let mut sockets: Vec<[SocketId; 4]> = Vec::new();

        for original in config.tiles {
            let adj = original.adjacencies;

            let base: [SocketId; 4] = [
                intern(&mut socket_ids, adj.north),
                intern(&mut socket_ids, adj.east),
                intern(&mut socket_ids, adj.south),
                intern(&mut socket_ids, adj.west),
            ];

            let turns = if original.rotate { 4 } else { 1 };
            let first_variant = sockets.len();

            for rotation in 0..turns {
                let mut rotated: [SocketId; 4] = [0; 4];
                for dir in Direction::ALL {
                    rotated[dir.rotate(rotation) as usize] = base[dir as usize];
                }

                if sockets[first_variant..].contains(&rotated) {
                    continue;
                }

                tiles.push(Tile {
                    name: original.name.clone(),
                    rotation,
                });
                sockets.push(rotated);
            }
        }

        if tiles.is_empty() {
            return Err("tileset contains no tiles".into());
        }
        if tiles.len() > MAX_TILES {
            return Err(format!(
                "tileset expands to {} tiles (including rotations), maximum is {MAX_TILES}",
                tiles.len()
            ));
        }

        let pairs: Vec<(SocketId, SocketId)> = config
            .pairs
            .into_iter()
            .map(|pair| {
                (
                    intern(&mut socket_ids, pair.a),
                    intern(&mut socket_ids, pair.b),
                )
            })
            .collect();

        let mut fits: Vec<SocketId> = (0..socket_ids.len()).collect();
        for (a, b) in pairs {
            fits[a] = b;
            fits[b] = a;
        }

        let n = tiles.len();
        let mut allowed: [Vec<Mask>; 4] = std::array::from_fn(|_| vec![0; n]);
        for dir in Direction::ALL {
            let d = dir as usize;
            for a in 0..n {
                allowed[d][a] = (0..n)
                    .filter(|&b| sockets[a][d] == fits[sockets[b][dir.opposite() as usize]])
                    .fold(0 as Mask, |m, b| m | (1 << b));
            }
        }

        Ok(Self {
            format: config.format,
            tiles,
            allowed,
        })
    }

    fn load_images(&self, dir: &Path) -> Result<Vec<RgbaImage>, Box<dyn Error>> {
        let mut by_name: HashMap<&str, RgbaImage> = HashMap::new();
        let mut images = Vec::with_capacity(self.tiles.len());

        for tile in &self.tiles {
            if !by_name.contains_key(tile.name.as_str()) {
                let path = dir.join(format!("{}.{}", tile.name, self.format));
                let img = image::open(&path)
                    .map_err(|e| format!("cannot load {}: {e}", path.display()))?
                    .to_rgba8();
                by_name.insert(&tile.name, img);
            }

            let base = &by_name[tile.name.as_str()];
            let rotated = match tile.rotation % 4 {
                0 => base.clone(),
                1 => imageops::rotate90(base),
                2 => imageops::rotate180(base),
                _ => imageops::rotate270(base),
            };
            images.push(rotated);
        }

        Ok(images)
    }
}

impl Grid {
    fn new(width: usize, height: usize, tile_count: usize, rng: &mut impl Rng) -> Self {
        let all: Mask = if tile_count >= 128 {
            Mask::MAX
        } else {
            (1 << tile_count) - 1
        };
        let cells: Vec<Cell> = (0..width * height)
            .map(|_| Cell { possible: all })
            .collect();
        let noise: Vec<u32> = (0..width * height).map(|_| rng.random()).collect();

        let mut grid = Self {
            width,
            height,
            cells,
            heap: BinaryHeap::new(),
            noise,
        };
        grid.rebuild_heap();
        grid
    }

    fn rebuild_heap(&mut self) {
        self.heap.clear();
        for i in 0..self.cells.len() {
            self.push_entropy(i);
        }
    }

    fn push_entropy(&mut self, i: usize) {
        let count = self.cells[i].possible.count_ones();
        if count > 1 {
            self.heap.push((Reverse(count), self.noise[i], i));
        }
    }

    fn neighbor(&self, i: usize, dir: Direction) -> Option<usize> {
        let (x, y) = (i % self.width, i / self.width);
        match dir {
            Direction::North => (y > 0).then(|| i - self.width),
            Direction::South => (y + 1 < self.height).then(|| i + self.width),
            Direction::West => (x > 0).then(|| i - 1),
            Direction::East => (x + 1 < self.width).then(|| i + 1),
        }
    }

    fn pop_lowest_entropy(&mut self) -> Option<usize> {
        while let Some((Reverse(count), _, i)) = self.heap.pop() {
            let current = self.cells[i].possible.count_ones();
            if current == count && current > 1 {
                return Some(i);
            }
        }
        None
    }

    fn collapse(&mut self, tileset: &Tileset, i: usize, choice: usize) -> Result<(), usize> {
        let mut bits = self.cells[i].possible;
        for _ in 0..choice {
            bits &= bits - 1;
        }
        let t = bits.trailing_zeros();
        self.cells[i].possible = 1 << t;
        self.propagate(tileset, i)
    }

    fn propagate(&mut self, tileset: &Tileset, start: usize) -> Result<(), usize> {
        let mut stack = vec![start];

        while let Some(i) = stack.pop() {
            let current = self.cells[i].possible;

            for dir in Direction::ALL {
                let Some(n) = self.neighbor(i, dir) else {
                    continue;
                };

                let mut support: Mask = 0;
                let mut bits = current;
                while bits != 0 {
                    let t = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    support |= tileset.allowed[dir as usize][t];
                }

                let old = self.cells[n].possible;
                let new = old & support;

                if new == old {
                    continue;
                }
                if new == 0 {
                    return Err(n);
                }

                self.cells[n].possible = new;
                self.push_entropy(n);
                stack.push(n);
            }
        }
        Ok(())
    }

    fn render(&self, images: &[RgbaImage]) -> Result<RgbaImage, String> {
        let (tw, th) = images.first().ok_or("no tile images")?.dimensions();
        if images.iter().any(|img| img.dimensions() != (tw, th)) {
            return Err("all tile images must have the same size".into());
        }

        let mut canvas = RgbaImage::new(self.width as u32 * tw, self.height as u32 * th);
        let unresolved = RgbaImage::from_pixel(tw, th, image::Rgba([255, 0, 255, 255]));

        for (i, cell) in self.cells.iter().enumerate() {
            let px = (i % self.width) as i64 * tw as i64;
            let py = (i / self.width) as i64 * th as i64;

            let tile_image = if cell.possible.count_ones() == 1 {
                &images[cell.possible.trailing_zeros() as usize]
            } else {
                &unresolved
            };

            imageops::replace(&mut canvas, tile_image, px, py);
        }

        Ok(canvas)
    }
}

fn parse_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(PathBuf, usize, usize), io::Error> {
    let mut args = args.into_iter();
    let program = args.next().unwrap_or_else(|| OsString::from("tiled-pic"));
    let usage = || {
        format!(
            "Usage: {} <tileset-directory> [width] [height]",
            program.to_string_lossy()
        )
    };

    let tileset_dir = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, usage()))?;
    let width = parse_dimension(args.next(), DEFAULT_WIDTH, "width")?;
    let height = parse_dimension(args.next(), DEFAULT_HEIGHT, "height")?;

    if args.next().is_some() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, usage()));
    }
    if width.checked_mul(height).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "grid dimensions are too large",
        ));
    }

    Ok((tileset_dir, width, height))
}

fn parse_dimension(
    value: Option<OsString>,
    default: usize,
    name: &str,
) -> Result<usize, io::Error> {
    let Some(value) = value else {
        return Ok(default);
    };
    let value = value
        .to_str()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{name} must be a positive integer"),
            )
        })?;
    Ok(value)
}

fn main() -> Result<(), Box<dyn Error>> {
    let (tileset_dir, width, height) = parse_args(env::args_os())?;
    let config_path = tileset_dir.join("config.toml");
    let text = fs::read_to_string(&config_path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read {}: {error}", config_path.display()),
        )
    })?;
    let config: Config = toml::from_str(&text)?;
    let tileset: Tileset = Tileset::from_config(config)?;
    let mut rng = rand::rng();
    let mut grid: Grid = Grid::new(width, height, tileset.tiles.len(), &mut rng);

    while let Some(i) = grid.pop_lowest_entropy() {
        let count = grid.cells[i].possible.count_ones() as usize;
        let choice = rng.random_range(0..count);
        match grid.collapse(&tileset, i, choice) {
            Ok(()) => {}
            Err(cell_index) => {
                let x = cell_index % grid.width;
                let y = cell_index / grid.width;
                return Err(std::io::Error::other(format!("Contradiction at ({x}, {y})")).into());
            }
        }
    }

    let images = tileset.load_images(&tileset_dir)?;
    let canvas = grid.render(&images)?;
    canvas.save("output.png")?;

    Ok(())
}
