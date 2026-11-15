# Interface icons

[Phosphor Icons](https://phosphoricons.com) by Helena Zhang and Tobias Fried, Copyright (c)
2023 Phosphor Icons, licensed under the [MIT License](LICENSE). Source: npm package
`@phosphor-icons/core` version 2.1.1 (repository https://github.com/phosphor-icons/core),
fetched through the jsDelivr npm mirror on September 10, 2026. The unmodified license is
`LICENSE` and is staged in both packages as `licenses/Phosphor-Icons-MIT.txt`.

Version-pinned Phosphor `assets/fill/*.svg` and `assets/bold/*.svg` files are scaled
to 56×56 pixels, filled white, and rasterized by `resvg` 0.45.1 into one transparent PNG atlas
with 64×64 cells (8 columns, 14 rows). `headphones-slash` is derived from `headphones-fill.svg`
by masking a diagonal knockout and adding a 16-unit round-capped stroke, matching the style of
Phosphor's own `*-slash` icons. The application tints glyphs at draw time; no icon font,
JavaScript or per-icon file is bundled.

Ten brand marks Phosphor does not ship (PlayStation, Battle.net, Epic Games, League of Legends,
Riot Games, Bungie, Roblox, Crunchyroll, eBay, Bluesky) come from
[Simple Icons](https://simpleicons.org) npm package `simple-icons` version 16.30.0
(repository https://github.com/simple-icons/simple-icons), released under
[CC0 1.0](LICENSE-SIMPLE-ICONS) and fetched through the same mirror on September 11, 2026.
Their 24-unit glyphs fill the whole view box, so they are drawn at 80 % of the cell glyph size
to match Phosphor's visual weight. Brand marks remain trademarks of their owners; Simple Icons'
legal disclaimer applies. The license file is staged in both packages as
`licenses/Simple-Icons-CC0.txt`.

One repository-drawn glyph, `thread.svg` (four slanted round-capped bars on the same 256-unit
grid), marks threads; it is rasterized with the Phosphor set and carries no upstream license.

- `atlas.png`: 512×896 RGBA, 112 cells, 54,792 bytes after lossless IDAT deflate level 9
  recompression (decoded bytes unchanged).
  SHA-256 `e781051967b933b8f746f7f356529881f2071e8d8a2dea2a2bd8fa5b57f6700b`.
- `index.tsv`: icon name, tab, zero-based cell; SHA-256
  `5a0c4e9336f4dcc1860d119ef850afdf96081bad8b2d395f12fd55fbd06293c4`.

Every upstream SVG's SHA-256 is pinned in `tools/generate-icons.py`, which refuses to build
from mismatching files. Regenerate from the repository root:

```sh
cargo install resvg --version 0.45.1 --root /tmp/resvg-tool
python3 tools/generate-icons.py --resvg /tmp/resvg-tool/bin/resvg
```

PNG compression bytes can vary with the resvg/png versions; decoded pixels and cell indices
are deterministic. The atlas remains below 256 KiB. UI picker regressions exercise the
bundled composer glyphs; icon cells are resolved by name from the generated index.
Repository-local SVG hashes use canonical LF line endings, and the generated index writes LF
on every platform, so Windows checkout conversion does not invalidate source verification.

The folder and open-folder glyphs are unmodified Phosphor `folder-fill.svg` and
`folder-open-fill.svg`, fetched from the same pinned 2.1.1 package on September 11, 2026.

The media-viewer caret and download glyphs are unmodified Phosphor `caret-left-bold.svg` and
`download-simple-bold.svg`, fetched from the same pinned 2.1.1 package on September 11, 2026.

The screen-share fullscreen glyph is unmodified Phosphor `corners-out-bold.svg`, fetched from the
same pinned 2.1.1 package on September 27, 2026.

The text context-menu glyphs are unmodified Phosphor `scissors-bold.svg`, `clipboard-text-bold.svg`
and `selection-all-bold.svg`, fetched from the same pinned 2.1.1 package on November 6, 2026.

The `serein-mark` cell is our own artwork, not an upstream icon: it is the solid silhouette of
the Serein chat-wave from `assets/brand/serein-mark.svg`, trimmed to its bounding box so it
matches Phosphor's glyph weight in the cell. It replaced the Discord brand mark that previously
occupied cell 58; no third-party application logo is bundled any more. `tools/generate-icons.py`
pins its SHA-256 like every upstream source and rasterizes it from the repository rather than a
package mirror.

The composer sticker glyph is unmodified Phosphor `sticker-fill.svg` from the same pinned 2.1.1
package. Its SHA-256 is `7b82c8037cc61e8c740821df59bb1df622ad83435f251383974e6c04285e83bb`.
It shares the existing atlas dimensions and GPU textures with the other composer controls.
