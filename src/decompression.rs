use zstd_safe::{DCtx, InBuffer, OutBuffer};

/// A zstd decompressor for PERF_RECORD_COMPRESSED records.
pub struct ZstdDecompressor {
    dctx: Option<DCtx<'static>>,
    buf: Option<Vec<u8>>,
}

impl Default for ZstdDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl ZstdDecompressor {
    pub fn new() -> Self {
        Self {
            dctx: None,
            buf: None,
        }
    }

    /// Decompress a chunk of zstd data and append it to `uncompressed_data`.
    pub fn decompress_into(
        &mut self,
        compressed_data: &[u8],
        uncompressed_data: &mut Vec<u8>,
    ) -> Result<(), std::io::Error> {
        let dctx = self.dctx.get_or_insert_with(DCtx::create);
        let buf = self.buf.get_or_insert_with(|| vec![0; 1024 * 1024]);

        let mut in_buffer = InBuffer::around(compressed_data);

        loop {
            let mut out_buffer = OutBuffer::around(&mut buf[..]);

            match dctx.decompress_stream(&mut out_buffer, &mut in_buffer) {
                Ok(_) => {
                    let out_len = out_buffer.pos();
                    uncompressed_data.extend_from_slice(&buf[..out_len]);
                    if out_len < buf.len() && in_buffer.pos() == compressed_data.len() {
                        break;
                    }
                }
                Err(code) => {
                    let error_name = zstd_safe::get_error_name(code);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Zstd decompression failed: {}", error_name),
                    ));
                }
            }
        }
        Ok(())
    }
}
