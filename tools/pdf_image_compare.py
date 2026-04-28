#!/usr/bin/env python
"""
Compare image output from two PDFs.

Requires:
  pip install pymupdf pillow

Examples:
  python tools/pdf_image_compare.py PrintCatalog_high.pdf mr_photo_high.pdf
  python tools/pdf_image_compare.py PrintCatalog_high.pdf mr_photo_high.pdf --sources High_photo1.png High_photo2.png
  python tools/pdf_image_compare.py PrintCatalog_high.pdf mr_photo_high.pdf --dpi 300
"""

from __future__ import annotations

import argparse
import io
from dataclasses import dataclass
from pathlib import Path

import fitz
from PIL import Image, ImageChops, ImageStat


@dataclass
class DiffStats:
    size: tuple[int, int]
    rmse: float
    mean_abs: float
    max_rgb: tuple[int, int, int]
    differing_pixels: int
    total_pixels: int
    bbox: tuple[int, int, int, int] | None

    @property
    def diff_percent(self) -> float:
        return (self.differing_pixels / self.total_pixels * 100.0) if self.total_pixels else 0.0


def open_rgb(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")


def crop_common(a: Image.Image, b: Image.Image) -> tuple[Image.Image, Image.Image]:
    width = min(a.width, b.width)
    height = min(a.height, b.height)
    return a.crop((0, 0, width, height)), b.crop((0, 0, width, height))


def diff_stats(a: Image.Image, b: Image.Image) -> DiffStats:
    a, b = crop_common(a.convert("RGB"), b.convert("RGB"))
    diff = ImageChops.difference(a, b)
    stat = ImageStat.Stat(diff)
    gray_hist = diff.convert("L").histogram()
    differing_pixels = sum(gray_hist[1:])
    total_pixels = a.width * a.height

    return DiffStats(
        size=a.size,
        rmse=sum(stat.rms) / len(stat.rms),
        mean_abs=sum(stat.mean) / len(stat.mean),
        max_rgb=tuple(channel[1] for channel in stat.extrema),
        differing_pixels=differing_pixels,
        total_pixels=total_pixels,
        bbox=diff.getbbox(),
    )


def print_stats(label: str, stats: DiffStats) -> None:
    print(
        f"  {label}: common={stats.size[0]}x{stats.size[1]}, "
        f"RMSE={stats.rmse:.4f}, mean={stats.mean_abs:.4f}, "
        f"maxRGB={stats.max_rgb}, diff={stats.diff_percent:.4f}%, bbox={stats.bbox}"
    )


def render_first_page(pdf_path: Path, dpi: int) -> Image.Image:
    doc = fitz.open(pdf_path)
    page = doc[0]
    pix = page.get_pixmap(matrix=fitz.Matrix(dpi / 72.0, dpi / 72.0), alpha=False)
    return Image.open(io.BytesIO(pix.tobytes("png"))).convert("RGB")


def print_pdf_inventory(pdf_path: Path) -> None:
    doc = fitz.open(pdf_path)
    print(f"\n{pdf_path}")
    print(f"  pages: {doc.page_count}")

    for page_index, page in enumerate(doc):
        rect = page.rect
        print(
            f"  page {page_index + 1}: {rect.width:.2f} x {rect.height:.2f} pt "
            f"({rect.width / 72:.3f} x {rect.height / 72:.3f} in)"
        )
        images = page.get_images(full=True)
        print(f"  embedded images: {len(images)}")

        for image_index, image_ref in enumerate(images, 1):
            xref = image_ref[0]
            image_info = doc.extract_image(xref)
            placements = page.get_image_rects(xref)
            preview_rects = "; ".join(
                f"({r.x0:.1f},{r.y0:.1f},{r.width:.1f}x{r.height:.1f})"
                for r in placements[:5]
            )
            if len(placements) > 5:
                preview_rects += f"; ... +{len(placements) - 5} more"

            print(
                f"    image {image_index}: xref={xref}, "
                f"{image_info.get('width')}x{image_info.get('height')}, "
                f"ext={image_info.get('ext')}, colorspace={image_info.get('colorspace')}, "
                f"bytes={len(image_info.get('image', b''))}, "
                f"placements={len(placements)} [{preview_rects}]"
            )


def reconstruct_embedded_groups(pdf_path: Path) -> dict[int, Image.Image]:
    """Stack embedded image strips by pixel width and xref order."""
    doc = fitz.open(pdf_path)
    page = doc[0]
    groups: dict[int, list[tuple[int, Image.Image]]] = {}

    for image_ref in page.get_images(full=True):
        xref = image_ref[0]
        image_info = doc.extract_image(xref)
        image = Image.open(io.BytesIO(image_info["image"])).convert("RGB")
        groups.setdefault(image.width, []).append((xref, image))

    reconstructed: dict[int, Image.Image] = {}
    for width, images in groups.items():
        images.sort(key=lambda item: item[0])
        total_height = sum(image.height for _, image in images)
        canvas = Image.new("RGB", (width, total_height), "white")
        y = 0
        for _, image in images:
            canvas.paste(image, (0, y))
            y += image.height
        reconstructed[width] = canvas

    return reconstructed


def rotated_variants(image_path: Path) -> list[tuple[str, Image.Image]]:
    image = open_rgb(image_path)
    return [
        (f"{image_path.name} original", image),
        (f"{image_path.name} rot90", image.rotate(90, expand=True)),
        (f"{image_path.name} rot180", image.rotate(180, expand=True)),
        (f"{image_path.name} rot270", image.rotate(270, expand=True)),
    ]


def compare_sources(groups: dict[str, dict[int, Image.Image]], source_paths: list[Path]) -> None:
    if not source_paths:
        return

    variants: list[tuple[str, Image.Image]] = []
    for source in source_paths:
        variants.extend(rotated_variants(source))

    print("\nBest reconstructed embedded image match to source images")
    for pdf_name, pdf_groups in groups.items():
        print(f"\n{pdf_name}")
        for width, image in sorted(pdf_groups.items(), reverse=True):
            candidates: list[tuple[float, str, DiffStats]] = []
            for label, source_image in variants:
                if min(image.width, source_image.width) < 100:
                    continue
                stats = diff_stats(image, source_image)
                candidates.append((stats.rmse, label, stats))

            if not candidates:
                continue

            _, label, stats = min(candidates, key=lambda candidate: candidate[0])
            print_stats(f"width {width} best {label}", stats)


def main() -> None:
    parser = argparse.ArgumentParser(description="Compare image output from two PDFs.")
    parser.add_argument("pdf_a", type=Path)
    parser.add_argument("pdf_b", type=Path)
    parser.add_argument("--sources", nargs="*", type=Path, default=[], help="Original source images.")
    parser.add_argument("--dpi", type=int, default=300, help="DPI used for rendered page comparison.")
    args = parser.parse_args()

    print("PDF metadata and embedded images")
    print_pdf_inventory(args.pdf_a)
    print_pdf_inventory(args.pdf_b)

    print("\nRendered first-page comparison")
    page_a = render_first_page(args.pdf_a, args.dpi)
    page_b = render_first_page(args.pdf_b, args.dpi)
    print(f"  {args.pdf_a}: rendered {page_a.width}x{page_a.height} px at {args.dpi} DPI")
    print(f"  {args.pdf_b}: rendered {page_b.width}x{page_b.height} px at {args.dpi} DPI")
    print_stats("rendered page", diff_stats(page_a, page_b))

    print("\nReconstructed embedded image groups")
    groups = {
        str(args.pdf_a): reconstruct_embedded_groups(args.pdf_a),
        str(args.pdf_b): reconstruct_embedded_groups(args.pdf_b),
    }
    for pdf_name, pdf_groups in groups.items():
        print(f"\n{pdf_name}")
        for width, image in sorted(pdf_groups.items(), reverse=True):
            print(f"  width group {width}: reconstructed {image.width}x{image.height}")

    print("\nPDF reconstructed embedded image vs PDF reconstructed embedded image")
    common_widths = set(groups[str(args.pdf_a)]).intersection(groups[str(args.pdf_b)])
    for width in sorted(common_widths, reverse=True):
        stats = diff_stats(groups[str(args.pdf_a)][width], groups[str(args.pdf_b)][width])
        print_stats(f"width {width}", stats)

    compare_sources(groups, args.sources)


if __name__ == "__main__":
    main()
