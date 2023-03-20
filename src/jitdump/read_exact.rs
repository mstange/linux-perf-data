pub trait ReadExactOrUntilEof {
    /// Reads until all of dest has been filled or until EOF has been reached.
    fn read_exact_or_until_eof(&mut self, dest: &mut [u8]) -> Result<usize, std::io::Error>;
}

impl<R: std::io::Read> ReadExactOrUntilEof for R {
    fn read_exact_or_until_eof(&mut self, mut dest: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut total_read = 0;
        while !dest.is_empty() {
            match self.read(dest)? {
                0 => break,
                n => {
                    total_read += n;
                    dest = &mut dest[n..];
                }
            }
        }
        Ok(total_read)
    }
}
