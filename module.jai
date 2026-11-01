// Jai bindings for the fontutils Rust crate (wraps fontdue, ttf-parser, sdfer and etagere).
//
// Build the crate with the slice_len_first feature to have the correct layout of slices for Jai

Font :: struct {}; // opaque
Atlas :: struct {}; // opaque

LineMetrics :: struct {
    ascent: float32;
    descent: float32;
    line_gap: float32;
}

GlyphKind :: enum u8 {
    NotContained :: 0;
    Whitespace :: 1;
    Default :: 2;
}

GlyphRequest :: struct {
    ch: u32;
    font_size: float32;
    sdf_radius: float32; // if 0, no sdf is generated and the plain rasterized coverage image is returned.
    sdf_pad: u32; // padding added on each of the 4 sides. With sdf_radius == 0 the image is NOT padded, but the metrics in GlyphInfo still are.
}

// Layout metrics of a glyph, in pixels at the requested font size. Already includes sdf_pad on every side.
GlyphInfo :: struct {
    xmin: float32;
    ymin: float32;
    width: float32;
    height: float32;
    advance: float32;
    kind: GlyphKind;
}

// Single channel image, 1 byte per pixel, row major, owned by Rust. Free with gray_image_free.
GrayImage :: struct {
    bytes: [] u8;
    width: u32;
    height: u32;
}

GlyphResponse :: struct {
    info: GlyphInfo;
    image: GrayImage; // empty for NotContained and Whitespace glyphs
}

AtlasAllocation :: struct {
    success: bool;
    id: u32; // pass to atlas_deallocate_tile to free the tile again
    x: u32; // top left corner of the inner (unpadded) region of the tile
    y: u32;
}


font_free :: (font: *Font) #foreign libfontutils;
font_get_line_metrics :: (font: *Font, font_size: float32) -> LineMetrics #foreign libfontutils;
font_get_kerning :: (font: *Font, left: u32, right: u32, font_size: float32) -> float32 #foreign libfontutils;

// response.image must be freed with gray_image_free.
font_get_glyph :: (font: *Font, request: GlyphRequest) -> GlyphResponse #foreign libfontutils;

atlas_create :: (width: u32, height: u32) -> *Atlas #foreign libfontutils;
atlas_free :: (atlas: *Atlas) #foreign libfontutils;
// Allocates a (width + 2*pad) x (height + 2*pad) tile. The returned x, y point to the inner region (offset by pad).
atlas_allocate_tile :: (atlas: *Atlas, width: u32, height: u32, pad: u32) -> AtlasAllocation #foreign libfontutils;
// Returns true if successful.
atlas_deallocate_tile :: (atlas: *Atlas, id: u32) -> bool #foreign libfontutils;

font_create_from_ttf_bytes :: (ttf_bytes: [] u8, error: *string) -> *Font #foreign libfontutils "font_create_from_ttf_bytes";
gray_image_free :: (image: *GrayImage) #foreign libfontutils "gray_image_free";

#scope_file

#if OS == .WINDOWS {
    libfontutils :: #library,no_static_library "lib/fontutils";
} else #if OS == .LINUX {
    libfontutils :: #library,no_static_library "lib/libfontutils";
}
