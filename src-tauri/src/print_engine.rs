/// Print Engine — Decode and rotate images, preserving original pixel data.
///
/// This module decodes the original image data (base64 from the frontend)
/// and applies only rotation. No resizing, cropping, or padding — the
/// printer driver handles all scaling via StretchDIBits.

use image::DynamicImage;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

/// Decode a base64 data URI (data:image/...;base64,...) into a DynamicImage
pub fn decode_data_uri(data_uri: &str) -> Result<DynamicImage, String> {
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

/// Decode and rotate an image. Returns the original pixels untouched
/// (except for rotation). No resizing, no cropping, no padding.
pub fn prepare_cell_image(cell: &crate::CellInfo) -> Result<DynamicImage, String> {
    let img = decode_data_uri(&cell.image_data)?;

    // Apply rotation only
    let rotated = match cell.rotation.rem_euclid(360) {
        90  => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _   => img,
    };

    Ok(rotated)
}
