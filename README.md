# PrintCatalog

High-fidelity photo grid printing — directly to your printer, bypassing the browser.

PrintCatalog is a professional-grade desktop application for arranging photos in precise grid layouts (passport photos, ID cards, contact sheets) and printing them at full resolution. By using a native Win32 GDI printing pipeline, it eliminates the quality loss and layout unpredictability common with browser-based printing.

## ✨ Key Features

- **🎯 Precise Grid Layouts**: Define dimensions in millimeters with sub-millimeter precision. Configure rows, columns, cell sizes, gaps, and page margins.
- **🖼️ Photo Catalog**: Manage a library of source photos for the current session. Drag-and-drop support (via file picker) for easy loading.
- **🛠️ Per-Cell Customization**:
  - **Scaling**: Choose between `Cover` (fill cell) or `Contain` (fit within cell).
  - **Alignment**: 9-point alignment grid (Top-Left to Bottom-Right).
  - **Rotation**: 90° increment rotations.
  - **Outlines**: Toggleable inward-stroke outlines (perfect for cutting guides).
- **🖨️ Native Print Engine**:
  - **Bypasses Browser**: Sends original pixel data directly to the OS print spooler.
  - **Win32 GDI Integration**: Uses `StretchDIBits` with `HALFTONE` interpolation for superior scaling.
  - **Custom Paper Support**: Automatically communicates custom dimensions to the printer via `DEVMODE` (A4, 4x6", Letter, etc.).
  - **Silent Printing**: Directly triggers the print job without a redundant print dialog.

## 🚀 How It Works

1.  **Configure**: Set your paper size (e.g., A4 or 4x6 photo paper) and grid matrix.
2.  **Import**: Add your high-resolution photos to the sidebar catalog.
3.  **Place**: Select a photo and click cells in the grid to "stamp" them.
4.  **Tune**: Click any placed photo to adjust its individual rotation, fit, or alignment.
5.  **Print**: Click **Print Direct**. The application composites the high-res images and sends them to your selected printer.

## 🛠️ Tech Stack

- **Frontend**: React 19, Vite, TailwindCSS 4, Framer Motion
- **Backend**: Rust (Tauri v2)
- **Image Processing**: `image` crate (lossless decoding and rotation)
- **Printing**: Win32 GDI via `windows` crate (Direct Device Context manipulation)
- **State**: Persistent local storage for your grid templates and session state

## 💻 Development

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- Windows 10/11 (required for Win32 Printing APIs and WebView2)

### Setup

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev
```

### Build

```bash
# Create a production installer
npm run tauri build
```
The installer will be generated in `src-tauri/target/release/bundle/`.

## 📜 License

MIT
