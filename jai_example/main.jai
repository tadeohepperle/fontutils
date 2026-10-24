#import "Basic";
#import "File";
#import "String";
#import "System";
#import "Math";
#import "Hash_Table";
#import "stb_image_write";
#import,dir "..";

// This is just some example usage of the Rust fontutils crate from Jai.
// For a real engine you probably do not want to couple fonts and atlasses together.
// A single atlas should be able to contains glyph tiles from multiple fonts

main :: () {
    // Resolve paths relative to the executable, so it works no matter where it is run from.
    dir := path_strip_filename(get_path_of_running_executable());

    bytes, ok := read_entire_file(tprint("%MarkoOne-Regular.default.ttf", dir));
    assert(ok);
    errstr: string;
    font := sdf_font_create(cast([] u8) bytes, SDF_FONT_SETTINGS_DEFAULT, *errstr);
    print("%\n", errstr);
    print("%\n", cast(*void) font);

    img := font.atlas_image;
    stbi_write_bmp(temp_c_string(tprint("%font_before.bmp", dir)), cast(s32) img.size.x, cast(s32) img.size.y, 1, img.bytes.data);
    print("%\n", sdf_font_get_or_add_glyph(font, #char "Å"));
    print("%\n", sdf_font_get_or_add_glyph(font, #char "§"));

    stbi_write_bmp(temp_c_string(tprint("%font_after.bmp", dir)), cast(s32) img.size.x, cast(s32) img.size.y, 1, img.bytes.data);
    print("%\n", sdf_font_get_horizontal_kerning(font, #char "V", #char "i"));

    print("\n\n\n");
    print("'T': %\n", sdf_font_get_or_add_glyph(font, #char "T"));
    print("'p': %\n", sdf_font_get_or_add_glyph(font, #char "p"));
    print("%\n", font.line_metrics);

    sdf_font_free(font);
}


UVec2 :: struct {
    x: u32;
    y: u32;
}

AtlasImage :: struct {
    size: UVec2;
    bytes: [] u8; // single channel grey image with 1 byte per pixel
}

SdfFontSettings :: struct {
    /// fontsize the sdf is rasterized at. 32 or 64 is recommended.
    font_size: u32;
    /// padding to each of the 4 dimensions for each glyph. A value of font_size / 8 is recommended.
    pad_size: u32;
    /// should be <= pad_size, if sdf_radius is 0, no sdf is computed at all and the original rasterized greyimage is put into the atlas
    sdf_radius: float32;
    /// should be a power of 2
    atlas_size: UVec2;
    // if true, the font will rasterize the majority of ascii characters already upon creation
    initialize_with_default_glyphs: bool;
}

SDF_FONT_SETTINGS_DEFAULT :: SdfFontSettings.{
    font_size = 64,
    pad_size = 16,
    sdf_radius = 16,
    atlas_size = UVec2.{ 1024, 1024 },
    initialize_with_default_glyphs = true,
};

// Glyph metrics (in pixels at settings.font_size, padding included) plus where to find the glyph in the atlas.
SdfGlyph :: struct {
    kind: GlyphKind;
    xmin: float32;
    ymin: float32;
    width: float32;
    height: float32;
    advance: float32;
    uv_min: Vector2;
    uv_max: Vector2;
}

SdfFont :: struct {
    settings: SdfFontSettings;
    font: *Font;
    atlas: *Atlas;
    atlas_image: AtlasImage;
    line_metrics: LineMetrics;
    glyphs: Table(u32, SdfGlyph);
    kerning: Table(u64, float32); // key: left << 32 | right
    // set to true whenever a glyph is added to atlas_image. Reset it after uploading the image to the GPU.
    has_atlas_image_changed: bool;
}

sdf_font_create :: (ttf_bytes: [] u8, settings := SDF_FONT_SETTINGS_DEFAULT, error: *string = null) -> *SdfFont {
    font := font_create_from_ttf_bytes(ttf_bytes, error);
    if !font return null;

    result := New(SdfFont);
    result.settings = settings;
    result.font = font;
    result.atlas = atlas_create(settings.atlas_size.x, settings.atlas_size.y);
    result.atlas_image.size = settings.atlas_size;
    result.atlas_image.bytes = NewArray(settings.atlas_size.x * settings.atlas_size.y, u8); // zero initialized
    result.line_metrics = font_get_line_metrics(font, xx settings.font_size);

    if settings.initialize_with_default_glyphs {
        DEFAULT_CHARS :: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.@#$%^&,!:;/?|(){}[]!+-_=* \n\t'\"><~`\\";
        for DEFAULT_CHARS sdf_font_get_or_add_glyph(result, it);
    }
    return result;
}

sdf_font_free :: (using sdf_font: *SdfFont) {
    font_free(font);
    atlas_free(atlas);
    array_free(atlas_image.bytes);
    deinit(*glyphs);
    deinit(*kerning);
    free(sdf_font);
}

sdf_font_get_or_add_glyph :: (using sdf_font: *SdfFont, ch: u32) -> SdfGlyph {
    cached := table_find_pointer(*glyphs, ch);
    if cached return cached.*;

    response := font_get_glyph(font, .{
        ch = ch,
        font_size = xx settings.font_size,
        sdf_radius = settings.sdf_radius,
        sdf_pad = settings.pad_size,
    });
    defer gray_image_free(*response.image);

    info := response.info;
    glyph := SdfGlyph.{
        kind = info.kind,
        xmin = info.xmin,
        ymin = info.ymin,
        width = info.width,
        height = info.height,
        advance = info.advance,
    };

    if info.kind == .Default {
        image := response.image;
        // An sdf image already contains the padding. A plain rasterized image does not, so let the atlas reserve it
        // around the tile, such that quads (which include the padding) sample empty pixels there.
        pad: u32 = ifx settings.sdf_radius == 0 then settings.pad_size else 0;
        alloc := atlas_allocate_tile(atlas, image.width, image.height, pad);
        assert(alloc.success, "font atlas is too small");

        for y: 0..image.height - 1 {
            src := image.bytes.data + y * image.width;
            dst := atlas_image.bytes.data + (alloc.y + y) * atlas_image.size.x + alloc.x;
            memcpy(dst, src, image.width);
        }

        atlas_w := cast(float32) atlas_image.size.x;
        atlas_h := cast(float32) atlas_image.size.y;
        glyph.uv_min = .{ (alloc.x - pad) / atlas_w, (alloc.y - pad) / atlas_h };
        glyph.uv_max = .{ (alloc.x + image.width + pad) / atlas_w, (alloc.y + image.height + pad) / atlas_h };
        has_atlas_image_changed = true;
    }

    table_add(*glyphs, ch, glyph);
    return glyph;
}

sdf_font_get_horizontal_kerning :: (using sdf_font: *SdfFont, left: u32, right: u32) -> float32 {
    key := (cast(u64) left << 32) | right;
    entry, newly_added := find_or_add(*kerning, key);
    if newly_added entry.* = font_get_kerning(font, left, right, xx settings.font_size);
    return entry.*;
}
