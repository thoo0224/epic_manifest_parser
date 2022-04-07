use byteorder::ReadBytesExt;

use crate::Result;

pub trait ReaderExt: ReadBytesExt {
    fn read_i32_le() -> Result<i32>;
}