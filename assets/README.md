# everytree icon

The icon combines a folder silhouette with five coloured treemap tiles. The artwork was created with the built-in image-generation tool, then resized and converted into Windows assets with Pillow. No API/CLI image-generation fallback was used.

- `icon-source.png`: original generated artwork with transparency.
- `everytree.ico`: 16, 20, 24, 32, 40, 48, 64, 96, 128, and 256 pixel Windows icon frames.
- `everytree.png`: transparent 256 pixel PNG.
- `everytree.rgba`: the same 256-by-256 pixels in straight RGBA order, embedded in the egui window without adding an image-decoder dependency.

`build.rs` compiles the ICO and version information into the Windows executable using the Windows SDK resource compiler. The app supplies the same artwork to its window/taskbar. Normal builds use the packaged assets; regenerating them requires `python scripts/package_icon.py` and Pillow.

## Generation prompt

Use case: logo-brand. Asset type: production Windows desktop application icon for everytree, a fast file-size treemap explorer. Create ONE finished square 1024x1024 icon asset on a genuinely transparent background (alpha), no mockup sheet. Subject: a distinctive front-facing folder silhouette whose face is an asymmetrical treemap. The folder has a substantial deep-charcoal rounded frame and a small warm-amber tab at its upper left. Inside its face, exactly five large crisply separated rounded rectangular tiles form an attractive treemap: one large vivid cyan-blue vertical rectangle on the left, a warm amber rectangle in the upper right, and three smaller teal-green, violet, and orange rectangles below it. All blocks are axis-aligned and precisely fitted together within the folder, separated by consistent dark channels. Make the geometry bold and simple, with restrained soft highlights on the tiles that echo cushion-shaded treemaps; clean mostly flat vector-like contours, polished Windows utility-app aesthetic. Folder fills approximately 86 percent of the canvas width, centered optically, balanced margins, strong silhouette and contrast at 16px/32px. No text, letters, numbers, magnifying glass, tree illustration, tiny details, busy texture, surrounding desktop, stand, perspective rotation, cast shadow beyond the icon, watermark or branding. Preserve fully transparent pixels outside the folder silhouette. Return only the icon artwork.