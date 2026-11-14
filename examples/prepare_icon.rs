use anyhow::{Context, Result, ensure};
use image::{ImageFormat, imageops::FilterType};
use std::{fs, io::Cursor};

fn main() -> Result<()> {
    let source = image::open("assets/jira.webp").context("Read supplied Jira icon")?;
    ensure!(source.width() == source.height(), "App icon must be square");
    source
        .resize_exact(64, 64, FilterType::Lanczos3)
        .save("assets/jira.png")?;
    let sizes = [16_u32, 20, 24, 32, 40, 48, 64, 128, 256];
    let images: Vec<_> = sizes
        .iter()
        .map(|&size| {
            let mut png = Cursor::new(Vec::new());
            source
                .resize_exact(size, size, FilterType::Lanczos3)
                .write_to(&mut png, ImageFormat::Png)?;
            Ok(png.into_inner())
        })
        .collect::<Result<_>>()?;
    let mut ico = vec![0, 0, 1, 0];
    ico.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + sizes.len() as u32 * 16;
    for (&size, png) in sizes.iter().zip(&images) {
        ico.extend([size as u8, size as u8, 0, 0]);
        ico.extend(1_u16.to_le_bytes());
        ico.extend(32_u16.to_le_bytes());
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in images {
        ico.extend(png);
    }
    fs::write("assets/app.ico", ico)?;
    Ok(())
}
