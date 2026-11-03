use crate::direction::Direction;
use image::{RgbaImage, imageops};
use rand::prelude::*;
use serde::Deserialize;
use std::{collections::HashMap, error::Error, path::Path};

type SocketId = usize;
pub(crate) type Mask = u128;

const MAX_TILES: usize = Mask::BITS as usize;
const ENTROPY_SCALE: f64 = 1_000_000.0;

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

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
struct OriginalTile {
    name: String,
    #[serde(default)]
    rotate: bool,
    #[serde(default = "default_weight")]
    weight: u32,
    adjacencies: Adjacencies,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
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
pub(crate) struct Tileset {
    format: String,
    tiles: Vec<Tile>,
    weights: Vec<u32>, // parallel to `tiles`, every entry >= 1
    allowed: [Vec<Mask>; 4],
}

fn set_bits(mut mask: Mask) -> impl Iterator<Item = usize> {
    std::iter::from_fn(move || {
        if mask == 0 {
            return None;
        }
        let tile = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        Some(tile)
    })
}

impl Tileset {
    pub(crate) fn from_config(config: Config) -> Result<Self, String> {
        fn intern(ids: &mut HashMap<String, SocketId>, socket: Socket) -> SocketId {
            let next = ids.len();
            *ids.entry(socket.0).or_insert(next)
        }

        let mut socket_ids: HashMap<String, SocketId> = HashMap::new();
        let mut tiles: Vec<Tile> = Vec::new();
        let mut weights: Vec<u32> = Vec::new();
        let mut sockets: Vec<[SocketId; 4]> = Vec::new();

        for original in config.tiles {
            if original.weight == 0 {
                return Err(format!(
                    "tile `{}` has weight 0; weights must be at least 1",
                    original.name
                ));
            }

            let weight = original.weight;
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
                weights.push(weight);
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

        weights.iter().try_fold(0u64, |acc, &weight| {
            acc.checked_add(u64::from(weight))
                .ok_or_else(|| "total tile weight exceeds u64::MAX".to_string())
        })?;
        debug_assert_eq!(tiles.len(), weights.len());

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
            weights,
            allowed,
        })
    }

    pub(crate) fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub(crate) fn allowed_for(&self, dir: Direction) -> &[Mask] {
        &self.allowed[dir as usize]
    }

    // Shannon entropy of the weighted possibilities, quantized to 1e-6 bits:
    // `H = log2(W) - (1/W) * sum(w_t * log2(w_t))` with `W = sum(w_t)`.
    pub(crate) fn entropy_units(&self, possible: Mask) -> u32 {
        let mut total: u64 = 0;
        let mut weighted_log_sum: f64 = 0.0;

        for t in set_bits(possible) {
            let weight = u64::from(self.weights[t]);
            let weight_f = weight as f64;
            total += weight;
            weighted_log_sum += weight_f * weight_f.log2();
        }

        debug_assert!(total > 0, "possible cell set must be non-empty");
        let total_f = total as f64;
        let entropy = total_f.log2() - weighted_log_sum / total_f;
        (entropy * ENTROPY_SCALE).round() as u32
    }

    // Picks a tile from `possible` with probability proportional to its weight.
    // Returns the absolute index into `self.tiles`. `possible` must be non-empty.
    pub(crate) fn choose_weighted(&self, possible: Mask, rng: &mut impl Rng) -> usize {
        let total: u64 = set_bits(possible).map(|t| u64::from(self.weights[t])).sum();

        debug_assert!(total > 0);
        let mut roll = rng.random_range(0..total);

        for t in set_bits(possible) {
            let weight = u64::from(self.weights[t]);
            if roll < weight {
                return t;
            }
            roll -= weight;
        }

        unreachable!("possible set is non-empty and all weights are at least 1")
    }

    pub(crate) fn load_images(&self, dir: &Path) -> Result<Vec<RgbaImage>, Box<dyn Error>> {
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
