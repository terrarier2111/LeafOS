use alloc::string::String;
use LeafOS::vga_buffer::ColoredString;

pub struct Shell {
    prompt: String,
}

impl Shell {

    // Uses vga_buffer char driver to check for empty current line in vga_buffer

    pub fn print(&self, text: &ColoredString) {
        
    }

}
