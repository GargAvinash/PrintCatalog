# Repository Guidelines

## Project Structure & Module Organization

This is a Tauri desktop app for high-quality photo grid printing.

- `src/`: React frontend. `App.tsx` contains most UI state, grid editing, image placement, and print-job creation.
- `src-tauri/src/`: Rust backend. `lib.rs` defines Tauri commands and print-job types, `printer.rs` contains Win32/GDI printing, and `print_engine.rs` handles image decoding/rotation.
- `src-tauri/icons/`: application icons used by Tauri packaging.
- `website/`: static marketing/help pages and website assets.
- `dist/` and `src-tauri/target/`: generated build outputs; do not edit manually.
- Root sample images/PDFs are used for manual print-quality comparison.

## Build, Test, and Development Commands

- `npm install`: install frontend and Tauri CLI dependencies.
- `npm run dev`: start the Vite frontend dev server.
- `npm run build`: build the frontend into `dist/`.
- `npm run preview`: preview the production frontend build.
- `npm run tauri dev`: run the full Tauri desktop app locally; agents must ask the user to run this because it is slow.
- `npm run tauri build`: create a production desktop package; agents must ask the user to run this because it is slow.
- `cd src-tauri && cargo test`: run Rust unit tests.
- `cd src-tauri && cargo fmt`: format Rust code.

## Coding Style & Naming Conventions

Use TypeScript/React for frontend changes and Rust 2024 for backend changes. Keep React components and interfaces in `PascalCase`; local variables, functions, and state setters use `camelCase`. Rust functions and fields use `snake_case`; serialized command payloads use `#[serde(rename_all = "camelCase")]` where they cross the frontend/backend boundary.

Prefer small, targeted changes in the print pipeline. The main goal is high-quality photo printing without compromising photo quality. Send original image data to the printer without app-side compression, resizing, color correction, or quality modification unless explicitly requested. Let the printer driver and printer settings handle device-specific color and output adjustments, matching the intent of NewSoft Presto Mr. Photo.

## Testing Guidelines

Rust tests live beside the implementation under `#[cfg(test)]`, as in `src-tauri/src/printer.rs`. Name tests by behavior, for example `cover_alignment_crops_wide_source_from_requested_side`. Run `cargo test` before submitting backend changes and `npm run build` before submitting frontend changes.

Manual print-quality changes should be checked with the high-resolution sample images and PDF outputs in the repository root, especially against Mr. Photo output.

## Commit & Pull Request Guidelines

Recent commits use Conventional Commit prefixes such as `fix:`, `chore:`, and `Bump version...`. Prefer concise, imperative commit messages, for example `fix: avoid duplicate image payloads in print jobs`.

Never create a git commit, tag, or other final git operation without first asking the user to manually test the application by running `npm run tauri dev`. If the user explicitly asks to commit, treat that as confirmation that manual testing is already complete and proceed without asking again. Agents may run checks such as `cargo test` and `npm run build` to verify compilation, but manual app testing is required before committing.

Pull requests should include a short summary, test results, and screenshots or sample PDFs when UI or print output changes. Link related issues when available, and call out any print-quality, DPI, color, or performance tradeoffs explicitly.

## Agent-Specific Instructions

Do not revert unrelated uncommitted changes. Treat printing behavior as high-stakes: inspect `mr_photo_high.pdf`, sample images, and generated PDFs before changing image scaling, compression, color, or DPI logic. Agents may run `npm run build`, but must ask the user before running `npm run tauri dev` or `npm run tauri build`.
