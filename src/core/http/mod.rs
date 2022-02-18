pub mod codes;
pub mod mime;
pub mod protocol;

pub enum Encoding {
    Gzip,
    Deflate,
}

#[derive(Debug)]
pub struct Range {
    pub from: Option<usize>,
    pub to: Option<usize>,
}
impl Range {
    pub fn new(from: Option<usize>, to: Option<usize>) -> Range {
        Range { from, to }
    }
}
