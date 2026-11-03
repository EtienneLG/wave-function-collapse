use crate::direction::Direction;
use crate::tileset::{Mask, Tileset};
use image::{RgbaImage, imageops};
use rand::prelude::*;
use std::{cmp::Reverse, collections::BinaryHeap};

type HeapEntry = (Reverse<u32>, u32, usize);

#[derive(Debug, Clone)]
struct Cell {
    possible: Mask,
}

#[derive(Debug)]
pub(crate) struct Grid {
    width: usize,
    height: usize,
    cells: Vec<Cell>, // flat 2D grid
    heap: BinaryHeap<HeapEntry>,
    noise: Vec<u32>,
}

impl Grid {
    pub(crate) fn new(width: usize, height: usize, tileset: &Tileset, rng: &mut impl Rng) -> Self {
        let tile_count = tileset.tile_count();
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
        grid.rebuild_heap(tileset);
        grid
    }

    pub(crate) fn width(&self) -> usize {
        self.width
    }

    pub(crate) fn possible(&self, i: usize) -> Mask {
        self.cells[i].possible
    }

    fn rebuild_heap(&mut self, tileset: &Tileset) {
        self.heap.clear();
        for i in 0..self.cells.len() {
            self.push_entropy(tileset, i);
        }
    }

    fn push_entropy(&mut self, tileset: &Tileset, i: usize) {
        let possible = self.cells[i].possible;
        if possible.count_ones() > 1 {
            self.heap
                .push((Reverse(tileset.entropy_units(possible)), self.noise[i], i));
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

    pub(crate) fn pop_lowest_entropy(&mut self, tileset: &Tileset) -> Option<usize> {
        while let Some((Reverse(entropy), _, i)) = self.heap.pop() {
            let possible = self.cells[i].possible;
            if possible.count_ones() > 1 && tileset.entropy_units(possible) == entropy {
                return Some(i);
            }
        }
        None
    }

    pub(crate) fn collapse(
        &mut self,
        tileset: &Tileset,
        i: usize,
        tile: usize,
    ) -> Result<(), usize> {
        debug_assert!(self.cells[i].possible & ((1 as Mask) << tile) != 0);
        self.cells[i].possible = (1 as Mask) << tile;
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
                    support |= tileset.allowed_for(dir)[t];
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
                self.push_entropy(tileset, n);
                stack.push(n);
            }
        }
        Ok(())
    }

    pub(crate) fn render(&self, images: &[RgbaImage]) -> Result<RgbaImage, String> {
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
