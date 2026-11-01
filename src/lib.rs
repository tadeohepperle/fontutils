/// Metrics associated with line positioning.
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(C)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}
impl LineMetrics {
    pub const ZERO: LineMetrics = LineMetrics {
        ascent: 0.0,
        descent: 0.0,
        line_gap: 0.0,
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn font_create_from_ttf_bytes(
    ttf_bytes: RawSlice,
    err: *mut RawString,
) -> *mut Font {
    match Font::new(ttf_bytes.typed::<u8>()) {
        Ok(font) => {
            RawString::set(err, "");
            return Box::leak(Box::new(font)) as *mut Font;
        }
        Err(err_str) => {
            RawString::set(err, err_str);
            return std::ptr::null_mut();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn font_free(font: *mut Font) {
    if font.is_null() {
        return;
    } else {
        let font_box = unsafe { Box::from_raw(font) };
        drop(font_box);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn font_get_line_metrics(font: *mut Font, font_size: f32) -> LineMetrics {
    if font.is_null() {
        return LineMetrics::ZERO;
    }
    let font: &Font = unsafe { &*font };
    font.get_line_metrics(font_size)
}

#[unsafe(no_mangle)]
pub extern "C" fn font_get_kerning(font: *mut Font, a_ch: u32, b_ch: u32, font_size: f32) -> f32 {
    if font.is_null() {
        return 0.0;
    }
    let Ok(a_ch) = char::try_from(a_ch) else {
        return 0.0;
    };
    let Ok(b_ch) = char::try_from(b_ch) else {
        return 0.0;
    };
    let font: &Font = unsafe { &*font };
    font.get_kerning(a_ch, b_ch, font_size)
}

#[unsafe(no_mangle)]
pub extern "C" fn font_get_glyph(font: *mut Font, req: GlyphRequest) -> GlyphResponse {
    if font.is_null() {
        return GlyphResponse::ZERO;
    }
    let font: &mut Font = unsafe { &mut *font };
    font.get_glyph(req)
}

pub struct Font {
    _font_bytes: Vec<u8>,
    font_face: ttf_parser::Face<'static>,
    font: fontdue::Font,
    reusable_sdf_buffers: Option<sdfer::esdt::ReusableBuffers>,
}

impl Font {
    fn get_line_metrics(&self, font_size: f32) -> LineMetrics {
        if let Some(h_line_metrics) = self.font.horizontal_line_metrics(font_size) {
            LineMetrics {
                ascent: h_line_metrics.ascent,
                descent: h_line_metrics.descent,
                line_gap: h_line_metrics.line_gap,
            }
        } else {
            LineMetrics::ZERO
        }
    }

    fn get_kerning(&self, a_ch: char, b_ch: char, font_size: f32) -> f32 {
        get_kerning(&self.font, &self.font_face, a_ch, b_ch, font_size).unwrap_or(0.0)
    }

    fn new(bytes: &[u8]) -> Result<Self, &'static str> {
        let font_bytes = bytes.to_vec();
        let font_face = ttf_parser::Face::parse(static_bytes(&font_bytes), 0)
            .map_err(|_| "Font Parsing Error")?;
        let fontdue_settings = fontdue::FontSettings::default();
        let font = fontdue::Font::from_bytes(&font_bytes[..], fontdue_settings)?;
        Ok(Font {
            _font_bytes: font_bytes,
            font_face,
            font,
            reusable_sdf_buffers: None,
        })
    }

    fn get_glyph(&mut self, request: GlyphRequest) -> GlyphResponse {
        let GlyphRequest {
            ch,
            font_size,
            sdf_radius,
            sdf_pad,
        } = request;

        let mut res: GlyphResponse = GlyphResponse::ZERO;
        let Ok(ch) = char::try_from(ch) else {
            return res;
        };

        let is_white_space = ch.is_whitespace();

        if !self.font.has_glyph(ch) {
            if is_white_space {
                res.info.kind = GlyphKind::Whitespace;
            }
            // if glyph not in font, mark this in the hashmap and return NotContained (except if is whitespace)
            return res;
        }

        let (f_metrics, rasterized_buf) = self.font.rasterize(ch, font_size as f32);
        if is_white_space {
            res.info.kind = GlyphKind::Whitespace;
            res.info.advance = f_metrics.advance_width;
            return res;
        }

        res.info = GlyphInfo {
            kind: GlyphKind::Default,
            xmin: f_metrics.bounds.xmin,
            ymin: f_metrics.bounds.ymin - f_metrics.bounds.height,
            width: f_metrics.bounds.width,
            height: f_metrics.bounds.height,
            advance: f_metrics.advance_width,
        };

        // otherwise, make the image:
        if sdf_radius <= 0.0 {
            // set the font already:
            res.image = GrayImage::new(
                rasterized_buf,
                f_metrics.width as u32,
                f_metrics.height as u32,
            );
        } else {
            // calculate the distance field:
            let mut gray_for_sdfer = sdfer::Image2d::<sdfer::Unorm8>::from_storage(
                f_metrics.width,
                f_metrics.height,
                rasterized_buf
                    .into_iter()
                    .map(|e| sdfer::Unorm8::from_bits(e))
                    .collect::<Vec<sdfer::Unorm8>>(),
            );
            let (sdfer_sdf_img, reuse) = sdfer::esdt::glyph_to_sdf(
                &mut gray_for_sdfer,
                sdfer::esdt::Params {
                    pad: sdf_pad as usize,
                    radius: sdf_radius,
                    cutoff: 0.5,
                    solidify: true,
                    preprocess: true,
                },
                self.reusable_sdf_buffers.take(),
            );

            self.reusable_sdf_buffers = Some(reuse);
            let (sdf_img_width, sdf_img_height) = (sdfer_sdf_img.width(), sdfer_sdf_img.height());
            res.image = GrayImage::new(
                sdfer_sdf_img
                    .data
                    .into_iter()
                    .map(sdfer::Unorm8::to_bits)
                    .collect(),
                sdf_img_width as u32,
                sdf_img_height as u32,
            );
            res.info.xmin -= sdf_pad as f32;
            res.info.ymin -= sdf_pad as f32;
            res.info.width += (2 * sdf_pad) as f32;
            res.info.height += (2 * sdf_pad) as f32;
        }

        return res;
    }
}

#[repr(C)]
pub struct GlyphRequest {
    pub ch: u32,
    pub font_size: f32,
    // if 0.0, no sdf is generated
    pub sdf_radius: f32,
    pub sdf_pad: u32,
}

fn get_kerning(
    font: &fontdue::Font,
    font_face: &ttf_parser::Face<'static>,
    a_ch: char,
    b_ch: char,
    font_size: f32,
) -> Option<f32> {
    let scale_factor = font.scale_factor(font_size);
    if let Some(kern) = get_horizontal_kern_from_gpos_table(font_face, a_ch, b_ch, scale_factor) {
        return Some(kern);
    }
    return font.horizontal_kern(a_ch, b_ch, font_size);
}

fn get_horizontal_kern_from_gpos_table(
    font_face: &ttf_parser::Face<'static>,
    a_ch: char,
    b_ch: char,
    scale_factor: f32,
) -> Option<f32> {
    let a_g_idx = font_face.glyph_index(a_ch)?;
    let b_g_idx = font_face.glyph_index(b_ch)?;
    let gpos = font_face.tables().gpos.as_ref()?;

    let x_advance_to_kerning = |x_advance: i16| -> Option<f32> {
        if x_advance == 0 {
            return None;
        } else {
            Some(x_advance as f32 * scale_factor)
        }
    };

    use ttf_parser::gpos::{PairAdjustment, PositioningSubtable};

    for lookup in gpos.lookups {
        for subtable in lookup.subtables.into_iter::<PositioningSubtable>() {
            match subtable {
                PositioningSubtable::Pair(PairAdjustment::Format1 { coverage, sets }) => {
                    if let Some(a_cov_idx) = coverage.get(a_g_idx) {
                        if let Some(pair_set) = sets.get(a_cov_idx) {
                            if let Some((rec_a, _rec_b)) = pair_set.get(b_g_idx) {
                                return x_advance_to_kerning(rec_a.x_advance);
                            }
                        }
                    }
                }
                PositioningSubtable::Pair(PairAdjustment::Format2 {
                    coverage,
                    classes,
                    matrix,
                }) => {
                    if coverage.get(a_g_idx).is_none() {
                        continue;
                    }
                    let classa = classes.0.get(a_g_idx);
                    let classb = classes.1.get(b_g_idx);
                    if let Some((rec_a, _rec_b)) = matrix.get((classa, classb)) {
                        return x_advance_to_kerning(rec_a.x_advance);
                    }
                }
                _ => {}
            }
        }
    }
    return None;
}

pub fn static_bytes<'a>(bytes: &'a [u8]) -> &'static [u8] {
    unsafe { std::mem::transmute(bytes) }
}

#[cfg(feature = "slice_len_first")]
#[repr(C)]
pub struct RawSlice {
    len: isize,
    ptr: *const (),
}
#[cfg(not(feature = "slice_len_first"))]
#[repr(C)]
pub struct RawSlice {
    ptr: *const (),
    len: isize,
}

#[cfg(feature = "slice_len_first")]
#[repr(C)]
pub struct RawString {
    len: isize,
    ptr: *const u8,
}
#[cfg(not(feature = "slice_len_first"))]
#[repr(C)]
pub struct RawString {
    ptr: *const u8,
    len: isize,
}

impl RawSlice {
    fn typed<T>(&self) -> &[T] {
        let align = std::mem::align_of::<T>();
        if self.ptr as usize % align != 0 {
            panic!("Unaligned pointer to type {}", std::any::type_name::<T>())
        }
        assert!(self.len >= 0);
        unsafe { std::slice::from_raw_parts(self.ptr as *const T, self.len as usize) }
    }
}

impl RawString {
    fn set(ptr: *mut RawString, value: &'static str) {
        unsafe {
            if let Some(ptr) = ptr.as_mut() {
                ptr.len = value.len() as isize;
                ptr.ptr = value.as_ptr();
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(C)]
pub struct AtlasParams {
    pub width: u32,
    pub height: u32,
}

#[unsafe(no_mangle)]
pub extern "C" fn atlas_create(width: u32, height: u32) -> *mut Atlas {
    let atlas = Atlas::new(width, height);
    Box::leak(Box::new(atlas)) as *mut Atlas
}

#[unsafe(no_mangle)]
pub extern "C" fn atlas_free(atlas: *mut Atlas) {
    if atlas.is_null() {
        return;
    } else {
        let atlas_box = unsafe { Box::from_raw(atlas) };
        drop(atlas_box);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn atlas_allocate_tile(
    atlas: *mut Atlas,
    width: u32,
    height: u32,
    pad: u32,
) -> AtlasAllocation {
    if atlas.is_null() {
        return AtlasAllocation::INVALID;
    };

    let atlas: &mut Atlas = unsafe { &mut *atlas };
    return atlas.allocate(width, height, pad);
}

// returns true if successful
#[unsafe(no_mangle)]
pub extern "C" fn atlas_deallocate_tile(atlas: *mut Atlas, id: u32) -> bool {
    if atlas.is_null() {
        return false;
    };

    let atlas: &mut Atlas = unsafe { &mut *atlas };
    return atlas.deallocate(id);
}

pub struct Atlas {
    pub allocator: etagere::AtlasAllocator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct AtlasAllocation {
    pub success: bool,
    pub id: u32,
    pub x: u32,
    pub y: u32,
}

impl AtlasAllocation {
    pub const INVALID: Self = Self {
        success: false,
        id: u32::MAX,
        x: 0,
        y: 0,
    };
}

impl Atlas {
    fn new(width: u32, height: u32) -> Self {
        let allocator = etagere::AtlasAllocator::new(etagere::size2(width as i32, height as i32));
        Self { allocator }
    }

    fn allocate(&mut self, width: u32, height: u32, pad: u32) -> AtlasAllocation {
        let size_for_etagere =
            etagere::Size::new((width + pad * 2) as i32, (height + pad * 2) as i32);
        if let Some(allocation) = self.allocator.allocate(size_for_etagere) {
            return AtlasAllocation {
                success: true,
                id: unsafe { std::mem::transmute(allocation.id) },
                x: allocation.rectangle.min.x as u32 + pad,
                y: allocation.rectangle.min.y as u32 + pad,
            };
        } else {
            return AtlasAllocation::INVALID;
        }
    }

    // returns true if deallocation was successful
    fn deallocate(&mut self, id: u32) -> bool {
        if id == u32::MAX {
            return false;
        }
        let alloc_id: etagere::AllocId = unsafe { std::mem::transmute(id) };
        self.allocator.deallocate(alloc_id);
        return true;
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default)]
pub enum GlyphKind {
    #[default]
    NotContained = 0,
    Whitespace = 1,
    Default = 2,
}

#[repr(C)]
pub struct GlyphResponse {
    info: GlyphInfo,
    image: GrayImage,
}

impl GlyphResponse {
    pub const ZERO: Self = Self {
        info: GlyphInfo::ZERO,
        image: GrayImage::ZERO,
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct GlyphInfo {
    pub xmin: f32,
    pub ymin: f32,
    pub width: f32,
    pub height: f32,
    pub advance: f32,
    pub kind: GlyphKind,
}

impl GlyphInfo {
    pub const ZERO: Self = Self {
        xmin: 0.0,
        ymin: 0.0,
        width: 0.0,
        height: 0.0,
        advance: 0.0,
        kind: GlyphKind::NotContained,
    };
}
#[repr(C)]
pub struct GrayImage {
    pub bytes: RawSlice,
    pub width: u32,
    pub height: u32,
}

impl GrayImage {
    pub const ZERO: Self = Self {
        bytes: RawSlice {
            len: 0,
            ptr: std::ptr::null(),
        },
        width: 0,
        height: 0,
    };

    fn new(img_bytes: Vec<u8>, width: u32, height: u32) -> Self {
        assert_eq!(img_bytes.len(), (width * height) as usize);
        let slice = Box::leak(Box::<[u8]>::from(img_bytes));

        Self {
            bytes: RawSlice {
                len: slice.len() as isize,
                ptr: slice.as_ptr() as *const (),
            },
            width,
            height,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn gray_image_free(img: *mut GrayImage) {
    if img.is_null() {
        return;
    };

    unsafe {
        let img: &mut GrayImage = &mut *img;

        if img.bytes.ptr.is_null() {
            return;
        }
        let slice: &mut [u8] = std::slice::from_raw_parts_mut(
            img.bytes.ptr as *const u8 as *mut u8,
            img.bytes.len as usize,
        );
        let boxed_slice = Box::<[u8]>::from_raw(slice);
        drop(boxed_slice);
        img.bytes.ptr = std::ptr::null_mut();
        img.bytes.len = 0;
        img.width = 0;
        img.height = 0;
    };
}
