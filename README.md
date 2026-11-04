# Wave Function Collapse in Rust

This project is a small Rust program that generates a tiled image from a set of PNG tiles and a simple adjacency configuration.

It works by reading a tileset folder, building a grid of possible tile matches from the rules in `config.toml`, and then filling the image with a random valid arrangement. The result is saved to `output.png` in the project directory.

## What it does

- Reads a tileset directory containing a `config.toml` file and matching PNG images
- Uses adjacency rules such as `north`, `east`, `south`, and `west`
- Rotates tiles when allowed
- Picks tiles using weighted randomness
- Writes the completed image to `output.png`

This is useful for generating patterned maps, puzzle textures, mazes, or other tile-based artwork.

## Quick start

From the project root:

```bash
cargo run -- tileset/3x3 50 50
```

This will generate a 50x50 image based on the included example tileset.

If you omit the size, it defaults to 100x100:

```bash
cargo run -- tileset/3x3
```

## Usage

```bash
tiled-pic <tileset-directory> [width] [height]
```

Arguments:

- `tileset-directory`: folder containing your tileset definition and tile images
- `width`: optional image width, must be a positive integer
- `height`: optional image height, must be a positive integer

## Included example tilesets

The repository includes sample tilesets under `tileset/`, for example:

- `tileset/3x3`
- `tileset/maze`
- `tileset/wang`

You can try one of them directly:

```bash
cargo run -- tileset/maze 40 40
```

## Creating your own tileset

Each tileset directory should contain:

- `config.toml`
- one PNG for each tile referenced in the config

A minimal config looks like this:

```toml
format = "png"

[[tiles]]
name = "void"
rotate = true
adjacencies = { north = "void", east = "void", south = "void", west = "void" }

[[tiles]]
name = "middle"
rotate = true
adjacencies = { north = "middle", east = "middle", south = "middle", west = "middle" }
```

The `adjacencies` field defines which socket a tile connects to on each side. Matching names are treated as compatible connections. You can add `weight` values to make certain tiles appear more often.

## Output

After running the program, it writes the generated image to:

```text
output.png
```

Open that file in an image viewer or editor to inspect the result.

## Building

To build the project without running it:

```bash
cargo build
```
