use zstd_safe::{DCtx, InBuffer, OutBuffer};

/// A zstd decompressor for PERF_RECORD_COMPRESSED records.
pub struct ZstdDecompressor {
    dctx: Option<DCtx<'static>>,
}

impl Default for ZstdDecompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl ZstdDecompressor {
    pub fn new() -> Self {
        Self { dctx: None }
    }

    /// Decompress a chunk of zstd data.
    pub fn decompress(&mut self, compressed_data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let dctx = self.dctx.get_or_insert_with(DCtx::create);

        let mut decompressed = vec![0; compressed_data.len() * 4];
        let mut in_buffer = InBuffer::around(compressed_data);
        let mut total_out = 0;

        while in_buffer.pos < in_buffer.src.len() {
            let available = decompressed.len() - total_out;
            let mut out_buffer = OutBuffer::around(&mut decompressed[total_out..]);

            match dctx.decompress_stream(&mut out_buffer, &mut in_buffer) {
                Ok(_) => {
                    total_out += out_buffer.pos();
                    if out_buffer.pos() == available {
                        decompressed.resize(decompressed.len() + compressed_data.len() * 4, 0);
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

        decompressed.truncate(total_out);

        Ok(decompressed)
    }
}
