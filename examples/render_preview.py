#!/usr/bin/env python3
"""Render the viewer's --json output to a PNG for terminal frame inspection.

Requires Pillow. Example:
  cargo run --example preview -- --json > screenshots/frame.json
  python examples/render_preview.py screenshots/frame.json screenshots/demo.png
"""
import argparse
import json
from pathlib import Path
import unicodedata
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("frame", type=Path)
parser.add_argument("output", type=Path)
parser.add_argument("--font", default="C:/Windows/Fonts/consola.ttf")
args = parser.parse_args()
frame = json.loads(args.frame.read_text(encoding="utf-8-sig"))
font = ImageFont.truetype(args.font, 16)
symbol_path = Path("C:/Windows/Fonts/seguisym.ttf")
symbols = ImageFont.truetype(str(symbol_path), 16) if symbol_path.is_file() else font
cell_width = round(font.getlength("M"))
cell_height, margin = 21, 18
foreground = {"text": "#d4d4d4", "muted": "#9098a3", "accent": "#6ab8ff", "error": "#f48771",
              "warning": "#e8c468", "success": "#6acb91", "cursor": "#14171c",
              "chart1": "#6acb91", "chart2": "#6ab8ff", "chart3": "#e8c468", "chart4": "#c297eb",
              "chart5": "#ef91b5", "chart6": "#68ccc8", "chart7": "#eaaa79", "chart8": "#b8c57d"}
background = {"": "#1e1e1e", "header": "#252a31", "panel": "#252a31", "selection": "#303e51",
              "cursor": "#6ab8ff", "match": "#685525"}
image = Image.new("RGB", (frame["width"] * cell_width + margin * 2, frame["height"] * cell_height + margin * 2), "#14171c")
draw = ImageDraw.Draw(image)
# Terminals join box-drawing strokes and block elements across the entire cell. Font rasterizers
# alone leave artificial gaps when the preview's line spacing exceeds the font.
connections = {"─": "lr", "━": "lr", "│": "ud", "┃": "ud", "┌": "dr", "┐": "dl",
               "└": "ur", "┘": "ul", "┬": "ldr", "┴": "lur",
               "├": "udr", "┤": "udl", "┼": "udlr"}
for row, line in enumerate(frame["lines"]):
    column = 0
    for span in line:
        color = foreground.get(span.get("role", ""), foreground["text"])
        bg = background.get(span.get("background", ""), background[""])
        for char in span["text"]:
            width = 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
            x, y = margin + column * cell_width, margin + row * cell_height
            draw.rectangle((x, y, x + width * cell_width - 1, y + cell_height - 1), fill=bg)
            if char in "▁▔":
                thickness = max(1, round(cell_height / 8))
                rail_y = y if char == "▔" else y + cell_height - thickness
                draw.rectangle((x, rail_y, x + cell_width - 1, rail_y + thickness - 1), fill=color)
                column += width
                continue
            if char in "╭╮╰╯":
                cx, cy = x + cell_width // 2, y + cell_height // 2
                radius = min(cell_width // 2, cell_height // 2)
                left, down = char in "╮╯", char in "╭╮"
                edge_x = cx - radius if left else cx + radius
                edge_y = cy + radius if down else cy - radius
                draw.line((x if left else x + cell_width - 1, cy, edge_x, cy), fill=color)
                draw.line((cx, edge_y, cx, y + cell_height - 1 if down else y), fill=color)
                box = (cx - 2 * radius if left else cx, cy if down else cy - 2 * radius,
                       cx if left else cx + 2 * radius, cy + 2 * radius if down else cy)
                start = {"╮": 270, "╭": 180, "╯": 0, "╰": 90}[char]
                draw.arc(box, start, start + 90, fill=color)
                column += width
                continue
            if char in connections:
                cx, cy = x + cell_width // 2, y + cell_height // 2
                for direction in connections[char]:
                    end = {"l": (x, cy), "r": (x + cell_width - 1, cy),
                           "u": (cx, y), "d": (cx, y + cell_height - 1)}[direction]
                    draw.line((cx, cy, *end), fill=color, width=2 if char in "━┃" else 1)
                column += width
                continue
            glyph_font = symbols if char in "∿✓⌕◆▾▸▎" else font
            glyph_x = x + max(0, (width * cell_width - glyph_font.getlength(char)) / 2)
            draw.text((glyph_x, y), char, font=glyph_font, fill=color)
            column += width
args.output.parent.mkdir(parents=True, exist_ok=True)
image.save(args.output)
print(args.output.resolve())
