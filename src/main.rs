mod cli;
mod direction;
mod grid;
mod tileset;

use std::{env, error::Error, fs, io};

use crate::grid::Grid;
use crate::tileset::{Config, Tileset};

fn main() -> Result<(), Box<dyn Error>> {
    let (tileset_dir, width, height) = cli::parse_args(env::args_os())?;
    let config_path = tileset_dir.join("config.toml");
    let text = fs::read_to_string(&config_path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read {}: {error}", config_path.display()),
        )
    })?;
    let config: Config = toml::from_str(&text)?;
    let tileset = Tileset::from_config(config)?;
    let mut rng = rand::rng();
    let mut grid = Grid::new(width, height, &tileset, &mut rng);

    let mut contradiction = None;

    while let Some(i) = grid.pop_lowest_entropy(&tileset) {
        let possible = grid.possible(i);
        let tile = tileset.choose_weighted(possible, &mut rng);
        if let Err(cell_index) = grid.collapse(&tileset, i, tile) {
            contradiction = Some(cell_index);
            break;
        }
    }

    let images = tileset.load_images(&tileset_dir)?;
    let canvas = grid.render(&images)?;
    canvas.save("output.png")?;

    if let Some(cell_index) = contradiction {
        let x = cell_index % grid.width();
        let y = cell_index / grid.width();
        eprintln!("warning: contradiction at ({x}, {y}); wrote partial image to output.png");
    }

    Ok(())
}
