# Static import fixtures

Twin_Cubes.glb is project-authored mathematical cube geometry, two static nodes
and two opaque base-color materials. No downloaded models/textures or Legacy
implementation are included. Regenerate with `python tools/make_static_fixtures.py`.
The manifest records byte sizes and SHA-256 hashes.

Bad_Truncated.glb is the same container with its final bytes removed. Import must
fail without replacing the current document/GPU scene or modifying either source.
These are technical fixtures, not the future diagnostic patient or a PBR reference.

Bad_Accessor.glb has an out-of-range POSITION reference. It exercises a malformed
input that could panic inside gltf-json 1.4.1 without application preflight. The
worker must return an error and successfully process the next valid request.

Color_Checker.glb contains two authored quads, a numerical 2×2 RGBA PNG, selected
UVs and linear vertex colors. One source image is shared by two textures with
different sampler settings; the left material is OPAQUE and the right is MASK.
Its mip-chain RGBA footprint is 20 bytes; decoded base pixels are 16 bytes.
Bad_Image.glb has malformed PNG data. Huge_Image_Header.glb declares 32768×32768
dimensions and must fail admission before allocating that image. Regenerate these
with `python tools/make_color_fixtures.py`. No user model/image is bundled.
