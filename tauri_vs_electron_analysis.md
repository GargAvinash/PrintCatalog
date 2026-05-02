# Tauri vs Electron vs Native — Print Quality Deep Dive

## Part 1: What Your DPI Setting Actually Does (Spoiler: Nothing)

Looking at your code, here's the truth:

```tsx
// GridConfig has a dpi field:
dpi?: number; // 300, 600, 1200

// It appears in the UI dropdown (line 506-516)
// But it is NEVER referenced in any rendering or print logic
```

**Your DPI dropdown is purely cosmetic.** It stores a number but doesn't affect anything. Here's what actually happens when you print:

```
┌─────────────────────────────────────────────────────────────────┐
│ Your photo: 3000×4000 pixels (original from camera)            │
│                           ↓                                     │
│ FileReader.readAsDataURL() → base64 data URI (lossless)        │
│                           ↓                                     │
│ <img src="data:..." style="width:35mm; height:45mm">           │
│                           ↓                                     │
│ Browser renders the <img> into a 35×45mm box on screen         │
│ (at ~96 DPI for screen = ~132×170 pixels on screen)            │
│                           ↓                                     │
│ window.print() → Browser re-renders for printer                │
│ The browser asks the printer: "What's your DPI?"               │
│ Printer says: "600 DPI"                                        │
│ Browser renders the 35mm cell at 600 DPI = 827×1063 pixels     │
│ Browser resamples your 3000×4000 image DOWN to 827×1063        │
│                           ↓                                     │
│ ⚠️ Quality loss happens HERE — browser's resampling algorithm  │
│ You have ZERO control over this step                           │
│                           ↓                                     │
│ Printer receives 827×1063 pixels for that cell                 │
└─────────────────────────────────────────────────────────────────┘
```

> [!CAUTION]
> **Standard document rendering is the bottleneck.** Even though your original photo has more than enough pixels, typical high-level print renderers often resample images using fixed internal algorithms. You can't control the interpolation method, and they don't send your original pixels to the printer.

### What the printer can actually handle

If you could send the original 3000×4000 pixel image and tell the printer "place this in a 35×45mm box", the printer would use its own high-quality downscaling (which printer drivers are heavily optimized for). That's what professional photo printing software does.

---

## Part 2: Tauri vs Electron — Head-to-Head

### Architecture Difference

````carousel
### Electron Architecture
```
┌──────────────────────────────────────┐
│            Electron App              │
│  ┌────────────────────────────────┐  │
│  │    Chromium (bundled, ~150MB)  │  │
│  │  ┌──────────────────────────┐  │  │
│  │  │  Your React UI (same)    │  │  │
│  │  └──────────────────────────┘  │  │
│  └────────────────────────────────┘  │
│  ┌────────────────────────────────┐  │
│  │    Node.js (bundled)           │  │
│  │    • sharp (image processing)  │  │
│  │    • pdf-to-printer (Win32)    │  │
│  │    • fs (file access)          │  │
│  └────────────────────────────────┘  │
│  Install size: ~200-300MB            │
└──────────────────────────────────────┘
```
<!-- slide -->
### Tauri Architecture
```
┌──────────────────────────────────────┐
│              Tauri App               │
│  ┌────────────────────────────────┐  │
│  │  WebView2 (pre-installed OS)  │  │
│  │  ┌──────────────────────────┐  │  │
│  │  │  Your React UI (same)    │  │  │
│  │  └──────────────────────────┘  │  │
│  └────────────────────────────────┘  │
│  ┌────────────────────────────────┐  │
│  │    Rust Backend                │  │
│  │    • image crate (processing)  │  │
│  │    • windows crate (Win32 API) │  │
│  │    • Direct GDI+ printing      │  │
│  └────────────────────────────────┘  │
│  Install size: ~5-10MB               │
└──────────────────────────────────────┘
```
````

### Comparison Matrix

| Criteria | Electron | Tauri |
|----------|----------|-------|
| **Install size** | ~200-300MB | ~5-10MB |
| **Language for backend** | JavaScript/Node.js | Rust |
| **Learning curve** | Low (you already know JS) | Medium (Rust is new) |
| **Print quality potential** | ✅ Identical | ✅ Identical |
| **Silent print (no dialog)** | ✅ `webContents.print({silent:true})` | ✅ Via Rust Win32 API |
| **Direct OS printer access** | Via `pdf-to-printer` or `edge-js` | Native via `windows` crate |
| **Image processing** | `sharp` (libvips, very fast) | `image` crate (fast, pure Rust) |
| **High-res image compositing** | ✅ sharp or node-canvas | ✅ image or tiny-skia crate |
| **Memory usage** | ~150-300MB RAM | ~30-80MB RAM |
| **Startup time** | 2-5 seconds | <1 second |
| **Auto-updates** | electron-updater (easy) | tauri-updater (easy) |
| **Build tooling maturity** | Very mature | Mature (v2 is stable) |
| **Your existing React code** | Works as-is | Works as-is |
| **Windows native feel** | Okay (it's Chrome in a window) | Better (uses native WebView) |

### For YOUR Use Case (High-Quality Photo Printing)

> [!IMPORTANT]
> **Print quality is identical between Electron and Tauri.** Both use a native print pipeline. The quality difference is in HOW you compose and send the image to the printer — and both can do it the same way.

The high-quality print pipeline (same in both):

```
Original photo (3000×4000 px)
        ↓
Backend (Node.js OR Rust) reads the ORIGINAL file bytes
        ↓
Compose onto a page canvas at FULL resolution:
  - A4 at 600 DPI = 4961 × 7016 pixels
  - Place the photo at exact mm position, keeping all original pixels
  - No browser resampling — direct pixel placement
        ↓
Send the composed full-res image to printer via OS API
        ↓
Printer driver handles final optimization for its hardware
```

---

## Part 3: Code Estimates

### Electron Approach

| Component | Lines | Notes |
|-----------|------:|-------|
| `main.js` (Electron main process) | ~80 | Window creation, IPC setup |
| `preload.js` (bridge) | ~30 | Expose print/file APIs to renderer |
| `print-engine.js` (image composition) | ~150 | Use `sharp` to compose full-res page |
| `printer-service.js` (OS printing) | ~80 | Use `pdf-to-printer` or GDI via `edge-js` |
| `package.json` + `electron-builder.json` | ~40 | Build/packaging config |
| **Modifications to `App.tsx`** | ~50 | Replace `window.print()`, use IPC for file paths |
| **New code total** | **~430** | |
| Your existing `App.tsx` (mostly unchanged) | ~808 | Minor modifications |
| **Total project** | **~1,240** | |

**Key dependencies:** `electron`, `electron-builder`, `sharp`, `pdf-to-printer`

**Simplified Electron print engine:**
```js
// print-engine.js — ~150 lines total, core logic shown
const sharp = require('sharp');

async function composePage(layout, imagePaths) {
  const { pageWidth, pageHeight, dpi, cells, grid } = layout;

  // Create page canvas at full DPI resolution
  const pageW = Math.round((pageWidth / 25.4) * dpi);  // mm → pixels at DPI
  const pageH = Math.round((pageHeight / 25.4) * dpi);

  // Start with blank white page
  let page = sharp({
    create: { width: pageW, height: pageH, channels: 4, background: 'white' }
  });

  // For each cell, resize original image and place it
  const composites = [];
  for (const [key, cell] of Object.entries(cells)) {
    const [row, col] = key.split('-').map(Number);
    const cellW = Math.round((grid.cellWidth / 25.4) * dpi);
    const cellH = Math.round((grid.cellHeight / 25.4) * dpi);
    const left = Math.round((grid.paddingLeft + col * (grid.cellWidth + grid.gapX)) / 25.4 * dpi);
    const top = Math.round((grid.paddingTop + row * (grid.cellHeight + grid.gapY)) / 25.4 * dpi);

    // Read ORIGINAL image at full resolution, resize to cell size
    const cellImage = await sharp(imagePaths[cell.imageId])
      .resize(cellW, cellH, { fit: cell.objectFit === 'contain' ? 'inside' : 'cover' })
      .toBuffer();

    composites.push({ input: cellImage, left, top });
  }

  return page.composite(composites).tiff({ quality: 100 }).toBuffer();
  // TIFF = lossless, sent directly to printer
}
```

### Tauri Approach

| Component | Lines | Notes |
|-----------|------:|-------|
| `main.rs` (Tauri setup) | ~40 | App initialization |
| `lib.rs` (command handlers) | ~60 | IPC command registration |
| `print_engine.rs` (image composition) | ~200 | Use `image` crate for full-res compositing |
| `printer.rs` (Win32 print API) | ~150 | Direct Win32 GDI printing |
| `Cargo.toml` | ~20 | Rust dependencies |
| `tauri.conf.json` | ~30 | Tauri configuration |
| **Modifications to `App.tsx`** | ~50 | Replace `window.print()`, use Tauri invoke |
| **New code total** | **~550** | |
| Your existing `App.tsx` (mostly unchanged) | ~808 | Minor modifications |
| **Total project** | **~1,360** | |

**Key dependencies:** `tauri`, `image`, `windows` (Win32 crate), `printpdf`

**Simplified Tauri print command:**
```rust
// print_engine.rs — core logic
use image::{RgbaImage, imageops};

#[tauri::command]
fn compose_and_print(layout: Layout, image_paths: Vec<String>) -> Result<(), String> {
    let dpi = layout.dpi as f64;
    let page_w = ((layout.page_width / 25.4) * dpi) as u32;
    let page_h = ((layout.page_height / 25.4) * dpi) as u32;

    let mut page = RgbaImage::from_pixel(page_w, page_h, image::Rgba([255,255,255,255]));

    for (key, cell) in &layout.cells {
        let (row, col) = parse_key(key);
        let cell_w = ((layout.cell_width / 25.4) * dpi) as u32;
        let cell_h = ((layout.cell_height / 25.4) * dpi) as u32;

        // Load ORIGINAL image at full resolution
        let img = image::open(&image_paths[&cell.image_id]).map_err(|e| e.to_string())?;
        let resized = img.resize_to_fill(cell_w, cell_h, imageops::FilterType::Lanczos3);

        let x = ((layout.padding_left + col * (layout.cell_width + layout.gap_x)) / 25.4 * dpi) as i64;
        let y = ((layout.padding_top + row * (layout.cell_height + layout.gap_y)) / 25.4 * dpi) as i64;

        imageops::overlay(&mut page, &resized, x, y);
    }

    send_to_printer(&page, layout.page_width, layout.page_height)?;
    Ok(())
}
```

---

## Part 4: Full Native Windows App — Is It Worth It?

### Code Estimate for Full Native

| Platform | UI + Layout | Print Engine | Total | Effort |
|----------|-------------|-------------|-------|--------|
| **C# WPF** | ~1,500 lines | ~300 lines | ~1,800 | 1-2 weeks |
| **C# WinUI 3** | ~1,800 lines | ~300 lines | ~2,100 | 1-2 weeks |
| **C++ Win32** | ~4,000 lines | ~400 lines | ~4,400 | 3-4 weeks |
| **Rust + egui** | ~1,500 lines | ~350 lines | ~1,850 | 2-3 weeks |

> [!WARNING]
> **Going full native means rebuilding your entire UI from scratch.** Your 808-line React app with drag-and-drop, modals, animations, grid preview — all of that needs to be re-implemented in a completely different UI framework. None of your current code is reusable.

### Print Quality: Native vs Electron/Tauri

```
Print Quality Comparison:

Full Native (C#/C++):
  Original pixels → Win32 GDI/GDI+ → Printer Driver → Printer
  Quality: ★★★★★

Electron (sharp + pdf-to-printer):
  Original pixels → sharp (libvips) → PDF → Printer Driver → Printer
  Quality: ★★★★★

Tauri (image crate + Win32):
  Original pixels → Rust image crate → Win32 GDI → Printer Driver → Printer
  Quality: ★★★★★

All three: IDENTICAL quality.
```

> [!IMPORTANT]
> **There is ZERO print quality difference between a native app and Electron/Tauri** when using a native rendering path for printing. The print pipeline is the same: read original image → compose at full resolution → send to printer via OS API. The UI framework doesn't touch the print data.

The only theoretical advantage of native is slightly lower overhead in the GDI call chain, but this is measured in microseconds — completely irrelevant for print quality.

### What native DOES give you (that you probably don't need):

- Slightly smaller install size (but Tauri is already ~5MB)
- Slightly lower RAM usage (but irrelevant for a printing app)
- Slightly faster startup (but Tauri is already <1 second)
- Access to advanced print APIs like XPS or Direct2D (but GDI via Tauri is equally good for photo printing)

---

## Part 5: Recommendation

### Go with Tauri. Here's why:

| Factor | Electron | Tauri | Native |
|--------|:--------:|:-----:|:------:|
| Print quality | ★★★★★ | ★★★★★ | ★★★★★ |
| Your code reuse | 95% | 95% | 0% |
| New code to write | ~430 lines | ~550 lines | ~1,800-4,400 lines |
| Install size | ~250MB | ~8MB | ~2MB |
| Development time | 1-2 days | 2-3 days | 1-4 weeks |
| Performance | Good | Excellent | Excellent |

**Tauri wins** because:
1. **Same print quality as everything else** — the print pipeline uses native OS APIs regardless
2. **95% of your React code is reused** — only the print handler changes
3. **~550 lines of new code** vs rebuilding everything from scratch for native
4. **5-10MB install** vs Electron's 250MB bloat
5. **Rust backend** gives you rock-solid image processing with `Lanczos3` resampling (the gold standard for downscaling photos)
6. **Direct Win32 access** via the `windows` crate — no intermediate libraries

### The key insight about image quality:

Your current app reads photos via `FileReader.readAsDataURL()` — this converts the image to a base64 string, which is **lossless** but loses the file path. In Tauri/Electron, you'd change this to:

1. **For the UI preview:** Still use data URLs (for the browser to display)
2. **For printing:** Store the **original file path** alongside the data URL, and pass it to the Rust/Node backend which reads the original file bytes directly

This means: **zero compression, zero resampling, zero quality loss** until the printer driver itself handles the final output.

> [!TIP]
> **Bottom line:** Tauri gives you the same print quality as a fully native app, reuses 95% of your code, and takes days instead of weeks to build. Native is simply not worth the effort for this use case.
