use fontdue::{Font, FontSettings};
use limine::Framebuffer;

use crate::conc_once_cell::ConcurrentOnceCellNoAlloc;

pub trait Renderer {

    fn new() -> Self;

    fn render(&self, frame_buffer: &Framebuffer, text: &str);

}

struct GenericRenderer {
    font: Font,
}

pub fn render_generic(frame_buffer: &Framebuffer, text: &str) {
    static GENERIC_RENDERER: ConcurrentOnceCellNoAlloc<GenericRenderer> = ConcurrentOnceCellNoAlloc::new();

    let renderer = GENERIC_RENDERER.get_or_init(|| GenericRenderer {
        font: {
            let font = include_bytes!("../resources/Generic.otf") as &[u8];
            Font::from_bytes(font, FontSettings::default()).unwrap()
        },
    }); // FIXME: rasterize text and render to frame buffer

}
