# PrintCatalog: Repository & Development Guidelines

This project, **PrintCatalog**, is a professional-grade desktop application designed for arranging photos in precise grid layouts (e.g., passport photos, ID cards) and printing them at full resolution using a native Win32 GDI printing engine.

## Project Overview

- **Purpose:** High-fidelity photo grid printing directly to system printers, prioritizing maximum image quality and speed.
- **Core Philosophy:** The primary goal is to print high-quality photos without any software-side quality compromise or color modification. We must send the raw pixel data directly to the printer driver. This allows the printer's own firmware and driver settings to handle color management and customization, mirroring the behavior of professional tools like **NewSoft Presto! Mr. Photo**.
- **Architecture:** 
    - **Frontend:** React 19 SPA built with Vite and TailwindCSS 4. It handles the UI for grid configuration, photo cataloging, and layout arrangement.
    - **Backend (Tauri v2):** Rust-based backend that manages system integration, image decoding/rotation, and direct communication with Windows printing APIs.
- **Communication:** IPC via Tauri commands. Payload sizes are strictly optimized to ensure the application remains fast and responsive even with 10MB+ images.

## Project Structure & Module Organization

- `src/`: React frontend source code. `App.tsx` contains most UI state, grid editing, image placement, and print-job creation.
- `src-tauri/src/`: Rust backend source code. `lib.rs` defines Tauri commands and print-job types, `printer.rs` contains Win32/GDI printing, and `print_engine.rs` handles image decoding/rotation.
- `src-tauri/icons/`: Application icons used by Tauri packaging.
- `website/`: Static marketing/help pages and website assets.
- `dist/` and `src-tauri/target/`: Generated build outputs; do not edit manually.
- Root sample images/PDFs are used for manual print-quality comparison.

## Building and Running

### Prerequisites
- Node.js (v18+)
- Rust (stable)
- Windows 10/11 (Required for Win32 Printing APIs)
- Visual Studio Build Tools with "Desktop development with C++"

### Key Commands
- `npm install`: install frontend and Tauri CLI dependencies.
- `npm run dev`: start the Vite frontend dev server.
- `npm run build`: build the frontend into `dist/`.
- `npm run tauri dev`: run the full Tauri desktop app locally (Starts Vite dev server + Tauri app).
- `npm run tauri build`: create a production desktop package. **CRITICAL:** NEVER execute this autonomously; it is time-intensive.
- `cd src-tauri && cargo test`: run Rust unit tests.
- `cd src-tauri && cargo fmt`: format Rust code.

## Development Conventions

- **System Print Dialog Requirement:** ALWAYS show the system print dialog (`PrintDlgA`) instead of printing silently. This is a critical requirement as users need to access "Printer Properties" to configure advanced driver-specific features like high-DPI modes, color management, and specialized media settings.
- **High-Quality Printing Mandate:** Raw pixel data must be passed to the printer via Win32 GDI (`StretchDIBits` with `HALFTONE`) without applying intermediate color profiles or compression. The printer driver must receive the data as "raw" as possible.
- **Performance Goal:** The application must aim for the speed and accuracy of **Presto! Mr. Photo**, handling high-resolution assets instantly.
- **IPC Optimization:** Never send redundant image data over the bridge. Use unique IDs and a central image map.
- **Image Processing:** Rotation is the only transformation allowed on the raw data before printing. No resizing or color correction.
- **Validation:** When modifying the print pipeline, ensure that the output matches the original file's fidelity.

## Coding Style & Naming Conventions

Use TypeScript/React for frontend changes and Rust 2024 for backend changes. Keep React components and interfaces in `PascalCase`; local variables, functions, and state setters use `camelCase`. Rust functions and fields use `snake_case`; serialized command payloads use `#[serde(rename_all = "camelCase")]` where they cross the frontend/backend boundary.

## Testing Guidelines

Rust tests live beside the implementation under `#[cfg(test)]`, as in `src-tauri/src/printer.rs`. Name tests by behavior, for example `cover_alignment_crops_wide_source_from_requested_side`. Run `cargo test` before submitting backend changes and `npm run build` before submitting frontend changes.

Manual print-quality changes should be checked with the high-resolution sample images and PDF outputs in the repository root, especially against Mr. Photo output.

## Commit & GitHub Integration

- **Conventional Commits:** Use prefixes such as `fix:`, `feat:`, `chore:`, and `docs:`.
- **GitHub Issue Integration:** To automatically close an issue, the commit message MUST include a closing keyword (e.g., `fixes`, `closes`, `resolves`) followed immediately by the issue number. Example: `feat: add boundary validation (fixes #1)`.
- **Testing & Committing Mandate:** NEVER perform a `git commit` or finalize a task without first asking the user to manually test the application by running `npm run tauri dev`. If the user explicitly asks to commit, treat that as confirmation that manual testing is already complete. Agents may run checks such as `cargo test`, but manual app testing is required.

## Agent-Specific Instructions

- Do not revert unrelated uncommitted changes. 
- Treat printing behavior as high-stakes: inspect `mr_photo_high.pdf`, sample images, and generated PDFs before changing image scaling, compression, color, or DPI logic. 
- Agents may run `npm run build`, but must ask the user before running `npm run tauri dev` or `npm run tauri build`.
