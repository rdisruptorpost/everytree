"""Package the generated icon artwork without changing its design.

Requires Pillow only when regenerating assets. Normal Rust builds use the
checked-in ICO and raw RGBA files and don't require Python or image libraries.
"""
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]

source = Image.open(ASSETS / "icon-source.png").convert("RGBA")
if source.width != source.height:
    raise ValueError("The source icon must be square.")
if source.getchannel("A").getextrema()[0] != 0:
    raise ValueError("The source icon must have a transparent background.")

source.save(ASSETS / "everytree.ico", format="ICO", sizes=[(n, n) for n in SIZES])
window_icon = source.resize((256, 256), Image.Resampling.LANCZOS)
window_icon.save(ASSETS / "everytree.png")
(ASSETS / "everytree.rgba").write_bytes(window_icon.tobytes())

with Image.open(ASSETS / "everytree.ico") as icon:
    assert icon.ico.sizes() == {(n, n) for n in SIZES}
    for size in icon.ico.sizes():
        frame = icon.ico.getimage(size)
        assert frame.mode == "RGBA"
        assert frame.getchannel("A").getextrema()[0] == 0
print("Packaged transparent Windows icons:", ", ".join(str(n) for n in SIZES))
