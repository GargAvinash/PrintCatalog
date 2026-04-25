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

/// Process a single image for a cell without resizing it.
/// It only crops or pads the image at its original resolution to match the cell's aspect ratio.
pub fn process_cell_image(
    cell: &crate::CellInfo,
    cell_w_mm: f64,
    cell_h_mm: f64,
) -> Result<RgbaImage, String> {
    let img = decode_data_uri(&cell.image_data)?;
    
    // Apply rotation
    let rotated = match cell.rotation.rem_euclid(360) {
        90  => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _   => img,
    };

    let src_w = rotated.width() as f64;
    let src_h = rotated.height() as f64;
    let target_aspect = cell_w_mm / cell_h_mm;
    let src_aspect = src_w / src_h;
    
    let (align_x, align_y) = parse_alignment(&cell.alignment);

    let processed = if cell.object_fit == "contain" {
        if src_aspect > target_aspect {
            // source is wider, pad height
            let new_h = (src_w / target_aspect).round() as u32;
            let mut canvas = RgbaImage::from_pixel(src_w as u32, new_h, Rgba([255, 255, 255, 255]));
            let offset_y = ((new_h as f64 - src_h) * align_y).round() as i64;
            imageops::overlay(&mut canvas, &rotated.to_rgba8(), 0, offset_y);
            canvas
        } else {
            // pad width
            let new_w = (src_h * target_aspect).round() as u32;
            let mut canvas = RgbaImage::from_pixel(new_w, src_h as u32, Rgba([255, 255, 255, 255]));
            let offset_x = ((new_w as f64 - src_w) * align_x).round() as i64;
            imageops::overlay(&mut canvas, &rotated.to_rgba8(), offset_x, 0);
            canvas
        }
    } else {
        // cover
        if src_aspect > target_aspect {
            // source is wider, crop width
            let new_w = (src_h * target_aspect).round() as u32;
            let crop_x = ((src_w - new_w as f64) * align_x).round() as u32;
            rotated.crop_imm(crop_x, 0, new_w, src_h as u32).to_rgba8()
        } else {
            // source is taller, crop height
            let new_h = (src_w / target_aspect).round() as u32;
            let crop_y = ((src_h - new_h as f64) * align_y).round() as u32;
            rotated.crop_imm(0, crop_y, src_w as u32, new_h).to_rgba8()
        }
    };

    Ok(processed)
}
