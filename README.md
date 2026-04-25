# PrintCatalog

High-quality photo grid printing — directly to your printer, bypassing the browser.

## What This Does

PrintCatalog is a desktop application (built with Tauri + React) for arranging photos in a grid layout and printing them at full resolution directly to your OS printer — **no browser print dialog, no quality loss**.

### How It Works

1. **Layout** — Design your photo grid (passport photos, ID cards, etc.) with precise mm dimensions
2. **Place** — Drop photos into cells with alignment, rotation, and fit controls
3. **Print** — Click "Print Direct" → photos are composited at your chosen DPI using Lanczos3 resampling → sent directly to the printer via Win32 GDI

### Why Not Just `window.print()`?

The browser's print pipeline resamples your images using its own internal algorithm (which you can't control) and forces you through a print dialog where wrong settings (margins, scaling, background graphics) can ruin your output. PrintCatalog bypasses all of this.

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- Windows 10+ (for WebView2 and Win32 printing APIs)

### Setup

```bash
npm install
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

This produces an installer in `src-tauri/target/release/bundle/`.

## Tech Stack

- **Frontend**: React 19 + Vite + TailwindCSS 4 + Motion
- **Backend**: Rust (Tauri v2)
- **Image Processing**: `image` crate with Lanczos3 resampling
- **Printing**: Win32 GDI via `windows` crate (StretchDIBits)
