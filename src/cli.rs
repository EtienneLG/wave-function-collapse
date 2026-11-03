use std::{ffi::OsString, io, path::PathBuf};

const DEFAULT_WIDTH: usize = 100;
const DEFAULT_HEIGHT: usize = 100;

pub(crate) fn parse_args(
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
