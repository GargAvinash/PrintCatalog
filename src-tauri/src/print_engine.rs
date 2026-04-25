/// Print Engine — High-quality image compositing at full DPI resolution.
///
/// This module takes the original image data (base64 from the frontend),
/// composes them onto a page-sized canvas at the exact DPI resolution,
/// and returns the composed image ready for the printer.
///
/// The key difference from browser printing: images are placed at their
/// native resolution (or the best resolution for the target DPI), using
/// Lanczos3 resampling — the gold standard for photo downscaling.
/// The browser's internal resampling is completely bypassed.

use image::{DynamicImage, RgbaImage, imageops, Rgba};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::PrintJob;

/// Decode a base64 data URI (data:image/...;base64,...) into a DynamicImage
fn decode_data_uri(data_uri: &str) -> Result<DynamicImage, String> {
    // Strip the data URI prefix: "data:image/png;base64," or similar
    let base64_data = data_uri
        .find(",")
        .map(|pos| &data_uri[pos + 1..])
        .ok_or_else(|| "Invalid data URI format".to_string())?;

    let bytes = BASE64
        .decode(base64_data)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    image::load_from_memory(&bytes)
        .map_err(|e| format!("Image decode error: {}", e))
}

/// Parse alignment string to (x_factor, y_factor) where 0.0 = start, 0.5 = center, 1.0 = end
fn parse_alignment(alignment: &str) -> (f64, f64) {
    match alignment {
        "top-left"      => (0.0, 0.0),
        "top-center"    => (0.5, 0.0),
        "top-right"     => (1.0, 0.0),
        "center-left"   => (0.0, 0.5),
        "center"        => (0.5, 0.5),
        "center-right"  => (1.0, 0.5),
        "bottom-left"   => (0.0, 1.0),
        "bottom-center" => (0.5, 1.0),
        "bottom-right"  => (1.0, 1.0),
        _               => (0.5, 0.5), // default center
    }
}

/// Convert mm to pixels at a given DPI
fn mm_to_px(mm: f64, dpi: u32) -> u32 {
    ((mm / 25.4) * dpi as f64).round() as u32
}

/// Resize an image for a cell using the specified fit mode and alignment.
/// Returns the final cell-sized image with proper cropping/padding.
fn resize_for_cell(
    img: &DynamicImage,
    cell_w: u32,
    cell_h: u32,
    object_fit: &str,
    alignment: &str,
    rotation: i32,
) -> RgbaImage {
    // Apply rotation first (on the source image)
    let rotated = match rotation.rem_euclid(360) {
        90  => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _   => img.clone(),
    };

    let src_w = rotated.width();
    let src_h = rotated.height();
    let (align_x, align_y) = parse_alignment(alignment);

    match object_fit {
        "contain" => {
            // Fit entire image inside cell, preserving aspect ratio (may have blank space)
            let scale_x = cell_w as f64 / src_w as f64;
            let scale_y = cell_h as f64 / src_h as f64;
            let scale = scale_x.min(scale_y);

            let new_w = (src_w as f64 * scale).round() as u32;
            let new_h = (src_h as f64 * scale).round() as u32;

            let resized = rotated.resize_exact(new_w, new_h, imageops::FilterType::Lanczos3);

            // Place on white background at alignment position
            let mut canvas = RgbaImage::from_pixel(cell_w, cell_h, Rgba([255, 255, 255, 255]));
            let offset_x = ((cell_w - new_w) as f64 * align_x).round() as i64;
            let offset_y = ((cell_h - new_h) as f64 * align_y).round() as i64;

            imageops::overlay(&mut canvas, &resized.to_rgba8(), offset_x, offset_y);
            canvas
        }
        _ => {
            // "cover" — fill the entire cell, cropping excess (no blank space)
            let scale_x = cell_w as f64 / src_w as f64;
            let scale_y = cell_h as f64 / src_h as f64;
            let scale = scale_x.max(scale_y);

            let new_w = (src_w as f64 * scale).round() as u32;
            let new_h = (src_h as f64 * scale).round() as u32;

            let resized = rotated.resize_exact(new_w, new_h, imageops::FilterType::Lanczos3);

            // Crop to cell size, aligned according to alignment
            let crop_x = ((new_w - cell_w) as f64 * align_x).round() as u32;
            let crop_y = ((new_h - cell_h) as f64 * align_y).round() as u32;

            let cropped = resized.crop_imm(crop_x, crop_y, cell_w, cell_h);
            cropped.to_rgba8()
        }
    }
}

/// Draw a 1-pixel black outline around a cell region on the page
fn draw_outline(page: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32) {
    let black = Rgba([0, 0, 0, 255]);
    let page_w = page.width();
    let page_h = page.height();

    for dx in 0..w {
        if x + dx < page_w {
            if y < page_h {
                page.put_pixel(x + dx, y, black);
            }
            if y + h - 1 < page_h {
                page.put_pixel(x + dx, y + h - 1, black);
            }
        }
    }
    for dy in 0..h {
        if y + dy < page_h {
            if x < page_w {
                page.put_pixel(x, y + dy, black);
            }
            if x + w - 1 < page_w {
                page.put_pixel(x + w - 1, y + dy, black);
            }
        }
    }
}

/// Compose a full-resolution page image from the print job.
///
/// This is the core function that replaces the browser's print rendering.
/// It creates a blank page at the exact DPI resolution, then places each
/// photo at its exact mm position using Lanczos3 resampling.
pub fn compose_page(job: &PrintJob) -> Result<RgbaImage, String> {
    let grid = &job.grid;
    let dpi = grid.dpi;

    // Create page canvas at full DPI resolution
    let page_w = mm_to_px(grid.page_width, dpi);
    let page_h = mm_to_px(grid.page_height, dpi);

    // Cell dimensions in pixels
    let cell_w = mm_to_px(grid.cell_width, dpi);
    let cell_h = mm_to_px(grid.cell_height, dpi);

    // Safety check
    if page_w > 20000 || page_h > 20000 {
        return Err(format!(
            "Page dimensions too large: {}x{} pixels. Max 20000. Try lowering DPI.",
            page_w, page_h
        ));
    }

    println!(
        "[PrintEngine] Composing page: {}x{} mm @ {} DPI = {}x{} pixels",
        grid.page_width, grid.page_height, dpi, page_w, page_h
    );
    println!(
        "[PrintEngine] Cell size: {}x{} mm = {}x{} pixels",
        grid.cell_width, grid.cell_height, cell_w, cell_h
    );

    // White page
    let mut page = RgbaImage::from_pixel(page_w, page_h, Rgba([255, 255, 255, 255]));

    // Place each cell
    for cell in &job.cells {
        // Decode the original image from base64
        let img = decode_data_uri(&cell.image_data)?;

        println!(
            "[PrintEngine] Cell ({},{}) — source image: {}x{} pixels, fit: {}, rotation: {}°",
            cell.row, cell.col, img.width(), img.height(), cell.object_fit, cell.rotation
        );

        // Resize/crop for the cell at full DPI resolution
        let cell_image = resize_for_cell(
            &img,
            cell_w,
            cell_h,
            &cell.object_fit,
            &cell.alignment,
            cell.rotation,
        );

        // Calculate position on page (in pixels)
        let x = mm_to_px(
            grid.padding_left + cell.col as f64 * (grid.cell_width + grid.gap_x),
            dpi,
        );
        let y = mm_to_px(
            grid.padding_top + cell.row as f64 * (grid.cell_height + grid.gap_y),
            dpi,
        );

        // Place on page
        imageops::overlay(&mut page, &cell_image, x as i64, y as i64);

        // Draw outline if requested
        if cell.outline {
            draw_outline(&mut page, x, y, cell_w, cell_h);
        }
    }

    println!("[PrintEngine] Page composed successfully");
    Ok(page)
}
