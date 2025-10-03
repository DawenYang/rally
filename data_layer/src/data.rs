use std::io::{self, Cursor, Read, Write};

#[derive(Debug)]
pub struct Data {
    pub field1: u32,
    pub field2: u16,
    pub filed3: String,
}

impl Data {
    pub fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        bytes.write(&self.field1.to_ne_bytes())?;
        bytes.write(&self.field2.to_ne_bytes())?;
        let field3_len = self.filed3.len() as u32;
        bytes.write(&field3_len.to_ne_bytes())?;
        bytes.extend_from_slice(self.filed3.as_bytes());
        Ok(bytes)
    }

    pub fn deserialize(cursor: &mut Cursor<&[u8]>) -> io::Result<Data> {
        let mut field1_bytes = [0u8;4];
    }
}
