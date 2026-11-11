//! Images and PDF documents shown in the response preview.

use std::sync::Arc;

use iced::widget::image::Handle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Pdf,
}

/// The body of a response that can be previewed (kept as it arrived).
#[derive(Debug, Clone)]
pub struct Media {
    pub kind: Kind,
    pub bytes: Arc<Vec<u8>>,
}

/// Recognises PNG, JPEG, GIF, WebP, BMP and PDF files by their first bytes.
pub fn sniff(bytes: &[u8]) -> Option<Kind> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n")
        || starts(&[0xFF, 0xD8, 0xFF])
        || starts(b"GIF87a")
        || starts(b"GIF89a")
        || starts(b"BM")
        || (bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
    {
        Some(Kind::Image)
    } else if bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|w| w == b"%PDF-")
    {
        Some(Kind::Pdf)
    } else {
        None
    }
}

/// The usual file extension for a body (`png`, `pdf`, …).
pub fn extension(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.starts_with(b"BM") {
        "bmp"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" {
        "webp"
    } else {
        "pdf"
    }
}

/// Zoom levels offered by the preview (1.0 = real size).
const ZOOMS: [f32; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

/// The next zoom level above (`up`) or below `current`.
pub fn step_zoom(current: f32, up: bool) -> f32 {
    if up {
        ZOOMS
            .iter()
            .copied()
            .find(|z| *z > current + 0.001)
            .unwrap_or(ZOOMS[ZOOMS.len() - 1])
    } else {
        ZOOMS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < current - 0.001)
            .unwrap_or(ZOOMS[0])
    }
}

// ───────────────────────────── images ─────────────────────────────

pub struct Picture {
    pub handle: Handle,
    pub width: u32,
    pub height: u32,
}

/// Pictures with more pixels than this are not decoded (they would take gigabytes).
const PIXEL_LIMIT: u64 = 100_000_000;

pub fn decode_image(bytes: &[u8]) -> Result<Picture, String> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| format!("Could not read the image: {e}"))?;
    if u64::from(width) * u64::from(height) > PIXEL_LIMIT {
        return Err(format!("The image is too large to preview ({width}×{height})."));
    }

    let decoded = image::load_from_memory(bytes).map_err(|e| format!("Could not read the image: {e}"))?;
    let rgba = decoded.to_rgba8();
    Ok(Picture {
        handle: Handle::from_rgba(width, height, rgba.into_raw()),
        width,
        height,
    })
}

// ───────────────────────────── PDF ─────────────────────────────

pub struct PdfDoc {
    pdf: hayro::hayro_syntax::Pdf,
    pub pages: usize,
    pub page: usize,
    pub zoom: f32,
    pub handle: Option<Handle>,
    /// Size in pixels of the rendered page.
    pub size: (u32, u32),
}

/// Largest side, in pixels, of a rendered page.
const PAGE_PIXEL_LIMIT: f32 = 6000.0;

impl PdfDoc {
    pub fn open(bytes: Arc<Vec<u8>>) -> Result<Self, String> {
        let pdf = hayro::hayro_syntax::Pdf::new(bytes).map_err(|e| match e {
            hayro::hayro_syntax::LoadPdfError::Decryption(_) => {
                "This PDF is protected by a password.".to_string()
            }
            _ => "Could not read this PDF.".to_string(),
        })?;
        let pages = pdf.pages().len();
        if pages == 0 {
            return Err("This PDF has no pages.".into());
        }
        let mut doc = Self {
            pdf,
            pages,
            page: 0,
            zoom: 1.0,
            handle: None,
            size: (0, 0),
        };
        doc.render();
        Ok(doc)
    }

    pub fn go_to(&mut self, page: usize) {
        let page = page.min(self.pages - 1);
        if page != self.page || self.handle.is_none() {
            self.page = page;
            self.render();
        }
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
        self.render();
    }

    /// Draws the current page at the current zoom (96 pixels per inch at 100 %).
    fn render(&mut self) {
        use hayro::vello_cpu::color::palette::css::WHITE;

        let pages = self.pdf.pages();
        let Some(page) = pages.get(self.page) else {
            self.handle = None;
            return;
        };
        let (w, h) = page.render_dimensions();
        let mut scale = self.zoom * 96.0 / 72.0;
        let longest = w.max(h) * scale;
        if longest > PAGE_PIXEL_LIMIT {
            scale *= PAGE_PIXEL_LIMIT / longest;
        }

        let cache = hayro::RenderCache::new();
        let pixmap = hayro::render(
            page,
            &cache,
            &hayro::hayro_interpret::InterpreterSettings::default(),
            &hayro::RenderSettings::default(),
            &hayro::PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        self.size = (width, height);
        self.handle = Some(Handle::from_rgba(
            width,
            height,
            pixmap.data_as_u8_slice().to_vec(),
        ));
    }
}

/// What the preview currently shows.
pub enum Preview {
    Image {
        picture: Picture,
        /// `None` = shrink to fit the panel.
        zoom: Option<f32>,
    },
    Pdf(PdfDoc),
}

impl Preview {
    pub fn open(media: &Media) -> Result<Self, String> {
        match media.kind {
            Kind::Image => Ok(Preview::Image {
                picture: decode_image(&media.bytes)?,
                zoom: None,
            }),
            Kind::Pdf => PdfDoc::open(media.bytes.clone()).map(Preview::Pdf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_files() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some(Kind::Image));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Kind::Image));
        assert_eq!(sniff(b"GIF89a.."), Some(Kind::Image));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Kind::Image));
        assert_eq!(sniff(b"%PDF-1.7\n"), Some(Kind::Pdf));
        assert_eq!(sniff(b"hello"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn zoom_steps() {
        assert_eq!(step_zoom(1.0, true), 1.25);
        assert_eq!(step_zoom(1.0, false), 0.75);
        assert_eq!(step_zoom(4.0, true), 4.0);
        assert_eq!(step_zoom(0.25, false), 0.25);
    }
}
