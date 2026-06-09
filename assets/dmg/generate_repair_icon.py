#!/usr/bin/env python3
from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate_repair_icon.py <output.png>")

    out = Path(sys.argv[1])
    size = 1024
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow)
    shadow_draw.rounded_rectangle((198, 122, 622, 740), radius=54, fill=(63, 42, 112, 42))
    shadow_draw.ellipse((530, 522, 880, 872), fill=(63, 42, 112, 42))
    image.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(26)))

    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((184, 112, 616, 728), radius=50, fill=(151, 104, 255, 255))
    draw.polygon([(500, 112), (616, 325), (562, 325), (500, 263)], fill=(216, 199, 255, 235))
    for y, width in [(290, 205), (386, 245), (482, 190)]:
        draw.rounded_rectangle((288, y, 288 + width, y + 42), radius=21, fill=(244, 239, 255, 245))

    draw.ellipse((518, 508, 858, 848), fill=(109, 40, 217, 255))
    draw.polygon(
        [
            (756, 534),
            (696, 594),
            (734, 632),
            (794, 572),
            (832, 610),
            (792, 722),
            (752, 762),
            (640, 802),
            (602, 764),
            (662, 704),
            (624, 666),
            (564, 726),
            (526, 688),
            (566, 576),
            (606, 536),
            (718, 496),
        ],
        fill=(255, 255, 255, 240),
    )

    out.parent.mkdir(parents=True, exist_ok=True)
    image.save(out, pnginfo=None)


if __name__ == "__main__":
    main()
