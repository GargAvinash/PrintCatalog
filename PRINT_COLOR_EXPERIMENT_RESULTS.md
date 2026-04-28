# Print Color Experiment Results

## Context

Users reported that PrintCatalog physical output was slightly darker than NewSoft Presto Mr. Photo. Commit `5c2745649845e4128db7244cf16be16681d54fb5` attempted to address this by enabling GDI ICM and neutralizing GDI `COLORADJUSTMENT` values while keeping high-quality HALFTONE scaling.

The later experiment removed app-side GDI color management while preserving full-resolution image data and printer-driver responsibility for final output.

## Experiment Change

Removed from `src-tauri/src/printer.rs`:

- `SetICMMode(...)`
- `GetColorAdjustment(...)`
- `SetColorAdjustment(...)`
- manual `COLORADJUSTMENT` gamma, brightness, contrast, reference black, and reference white overrides

Kept:

- full-resolution source image path
- image caching
- crop/placement logic
- `SetStretchBltMode(hdc, HALFTONE)`
- `SetBrushOrgEx(hdc, 0, 0, None)`
- `LOGPIXELSX` / `LOGPIXELSY` DPI scaling

## PDF Comparison Results

Compared:

- Previous app output: `PrintCatalog_high.pdf`
- Experimental output: `PrintCatalog_high_codex_experiment.pdf`
- Reference output: `mr_photo_high.pdf`
- Sources: `High_photo1.png`, `High_photo2.png`

Lower RMSE is closer to the original source image.

| Image | Previous PrintCatalog RMSE | Experimental PrintCatalog RMSE | Mr. Photo RMSE |
|---|---:|---:|---:|
| `High_photo2.png` rotated | `32.7321` | `3.1910` | `3.1931` |
| `High_photo1.png` | `15.2620` | `4.8028` | `4.8054` |

## Interpretation

The experimental output is dramatically closer to the original source images and effectively matches Mr. Photo in the PDF-based comparison.

The earlier `5c27456` color-shift fix is now only partially retained:

- Retained: HALFTONE mode, `SetBrushOrgEx`, and true DPI scaling via `LOGPIXELSX` / `LOGPIXELSY`.
- Removed in experiment: app-side ICM and `COLORADJUSTMENT` manipulation.

This suggests that the ICM/color-adjustment portion of `5c27456` was likely responsible for the remaining source-color difference in Microsoft Print to PDF output, despite being configured with neutral-looking values.

## Physical Print Follow-Up

The PDF comparison strongly indicates improved source fidelity, but darkness must still be validated on a real printer. The expected behavior is that PrintCatalog now sends more neutral/original image data and lets the printer driver apply any user-selected printer settings, closer to Mr. Photo’s behavior.
